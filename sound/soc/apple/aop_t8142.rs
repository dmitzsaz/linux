// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Apple T8142 (M5) AOP audio
//!
//! The speaker and headphone jack streams are T6041's (`aop_spkr`): the same
//! services, base-ns ADMAC channels, frames and power sequence. As T8140's,
//! this firmware takes each power request with its time in the AOP timebase.
//!
//! The microphone array (`hpai`, ADT iop-audio-controller/audio-hp-mic) is
//! T8140's: the firmware runs the PDM part itself, so there is no PDM or
//! decimator setup and no `lpai` here, and delivers 48 kHz frames of three
//! float32 channels on leap-s ADMAC RX0. Three words are not a frame the ADMAC
//! can mark, so the channel moves a plain word stream, and a channel lines up
//! with the microphones only when the firmware starts producing into it:
//!
//! * open requests `pw0 `, which raises the fabric the channel's registers
//!   need, before it requests the channel;
//! * the trigger starts the DMA and queues `pwrd` behind it (a sleeping
//!   firmware call), so the firmware starts on a running channel, at a frame;
//! * a prepare of a stream the firmware runs (a restart after an overrun, or
//!   after a system sleep) takes it back to `pw0 ` first, so the next start
//!   begins on a frame again. The PCM cannot pause, and it resumes through
//!   prepare, for the same reason;
//! * close releases the channel, then returns the firmware to `pw0 `.

use kernel::{
    bindings, device,
    error::from_err_ptr,
    platform,
    prelude::*,
    soc::apple::aop::from_fourcc,
    sync::{atomic::Relaxed, Arc},
    types::ForeignOwnable,
    workqueue::{self, Work, WorkItem},
};

use super::{
    aop_spkr, pcm_open_dma, SndSocAopData, SndSocAopDriver, SndSocAopHwConfig, AUDIO_DEV_HPAI,
    POWER_STATE_ON,
};

use core::{ops::Range, ptr};

const POWER_STATE_PW0: u32 = from_fourcc(b"pw0 ");

/// The PDM fields are unused: the firmware runs the microphones' PDM part.
static HW_CFG_T8142: SndSocAopHwConfig = SndSocAopHwConfig {
    bytes_per_sample: 4,
    hf_decimator: false,
    t6040_flow: false,
    power_timestamp: true,
    mic_dma: c"hpai",
    power_domains: &[c"mca0", c"mca2", c"leap_s_a", c"leap_mca", c"leap_rx0"],
};

/// A named host-controlled serializer or LEAP power leaf.
pub(super) struct PowerDomain {
    dev: ptr::NonNull<bindings::device>,
}

// SAFETY: genpd serializes runtime PM; the handle stays attached until Drop.
unsafe impl Send for PowerDomain {}
// SAFETY: all operations on the domain are serialized by runtime PM.
unsafe impl Sync for PowerDomain {}

impl PowerDomain {
    pub(super) fn attach(dev: &device::Device, name: &CStr) -> Result<Self> {
        // SAFETY: the device is live and name is NUL-terminated.
        let pd = from_err_ptr(unsafe {
            bindings::dev_pm_domain_attach_by_name(dev.as_raw(), name.as_char_ptr())
        })?;
        Ok(Self {
            dev: ptr::NonNull::new(pd).ok_or(ENODEV)?,
        })
    }

    fn get(&self) -> Result<()> {
        // SAFETY: this virtual device remains attached for the handle's lifetime.
        let ret = unsafe { bindings::pm_runtime_get_sync(self.dev.as_ptr()) };
        if ret < 0 {
            // SAFETY: a failed get still increments the usage count. Clear the
            // failed resume so a later open can retry after the fabric recovers.
            unsafe {
                bindings::pm_runtime_put_noidle(self.dev.as_ptr());
                bindings::__pm_runtime_set_status(
                    self.dev.as_ptr(),
                    bindings::rpm_status_RPM_SUSPENDED as u32,
                );
            }
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    fn put(&self) {
        // SAFETY: balances a successful get while the domain remains attached.
        unsafe { bindings::pm_runtime_put_sync(self.dev.as_ptr()) };
    }
}

impl Drop for PowerDomain {
    fn drop(&mut self) {
        // SAFETY: attached above; stream shutdown has balanced all references.
        unsafe { bindings::dev_pm_domain_detach(self.dev.as_ptr(), true) };
    }
}

impl SndSocAopData {
    pub(super) fn power_up(&self, domains: Range<usize>) -> Result<()> {
        // Other variants let the firmware manage the entire stream.
        if self.power_domains.is_empty() {
            return Ok(());
        }
        for i in domains.clone() {
            if let Err(e) = self.power_domains[i].get() {
                self.power_down(domains.start..i);
                return Err(e);
            }
        }
        Ok(())
    }

    pub(super) fn power_down(&self, domains: Range<usize>) {
        if self.power_domains.is_empty() {
            return;
        }
        for i in domains.rev() {
            self.power_domains[i].put();
        }
    }

    /// Returns the firmware to `pw0 ` once no start is pending.
    fn hpai_stop(&self) -> Result<()> {
        self.hpai_want_run.store(false, Relaxed);
        // SAFETY: the work item lives as long as `self`; flushing waits for a
        // queued start to finish.
        unsafe { bindings::flush_work(Work::raw_get(&self.hpai_start)) };
        if self.hpai_running.load(Relaxed) {
            self.set_audio_power(POWER_STATE_PW0, 0)?;
            self.hpai_running.store(false, Relaxed);
        }
        Ok(())
    }
}

/// The microphone array's `pwrd`, queued by the capture trigger: the ADMAC
/// channel runs by then.
impl WorkItem for SndSocAopData {
    type Pointer = Arc<Self>;

    fn run(this: Arc<Self>) {
        if !this.hpai_want_run.load(Relaxed) || this.hpai_running.load(Relaxed) {
            return;
        }
        match this.set_audio_power(POWER_STATE_ON, 0) {
            Ok(()) => this.hpai_running.store(true, Relaxed),
            Err(e) => dev_err!(this.dev, "hpai: start failed: {:?}\n", e),
        }
    }
}

/// # Safety
///
/// `substream` must be a live substream of the microphone card, whose private
/// data is the driver's `Arc`.
unsafe fn hpai_data<'a>(
    substream: *mut bindings::snd_pcm_substream,
) -> kernel::sync::ArcBorrow<'a, SndSocAopData> {
    // SAFETY: by the caller's contract.
    unsafe { Arc::<SndSocAopData>::borrow((*substream).private_data.cast()) }
}

/// # Safety
///
/// Called by ALSA with a live substream of the microphone card.
unsafe extern "C" fn hpai_open(substream: *mut bindings::snd_pcm_substream) -> i32 {
    // SAFETY: by the caller's contract.
    let data = unsafe { hpai_data(substream) };
    if let Err(e) = data.set_audio_power(POWER_STATE_PW0, 0) {
        dev_err!(data.dev, "hpai: pw0 failed: {:?}\n", e);
        return e.to_errno();
    }
    if let Err(e) = data.power_up(2..5) {
        return e.to_errno();
    }
    // SAFETY: by the caller's contract. Pause and resume are not advertised:
    // the three-word stream must restart with the firmware on a fresh frame.
    let ret = unsafe { pcm_open_dma(&data, substream) };
    if ret < 0 {
        data.power_down(2..5);
    }
    ret
}

/// # Safety
///
/// Called by ALSA with a live substream of the microphone card.
unsafe extern "C" fn hpai_close(substream: *mut bindings::snd_pcm_substream) -> i32 {
    // SAFETY: by the caller's contract.
    let data = unsafe { hpai_data(substream) };
    // SAFETY: the dmaengine runtime was set up in `hpai_open`. The channel's
    // callbacks are drained before the firmware stops producing into it.
    let ret = unsafe { bindings::snd_dmaengine_pcm_close_release_chan(substream) };
    let stopped = data.hpai_stop();
    data.power_down(2..5);
    if ret < 0 {
        ret
    } else {
        stopped.map_or_else(|e| e.to_errno(), |()| 0)
    }
}

/// # Safety
///
/// Called by ALSA with a live substream of the microphone card and live
/// parameters.
unsafe extern "C" fn hpai_hw_params(
    substream: *mut bindings::snd_pcm_substream,
    params: *mut bindings::snd_pcm_hw_params,
) -> i32 {
    // SAFETY: the dmaengine runtime was set up in `hpai_open`.
    let chan = unsafe { bindings::snd_dmaengine_pcm_get_chan(substream) };
    let mut config = bindings::dma_slave_config::default();
    // SAFETY: the substream and the parameters are live for the op.
    let ret = unsafe { bindings::snd_hwparams_to_dma_slave_config(substream, params, &mut config) };
    if ret < 0 {
        return ret;
    }
    // A plain word stream: the ADMAC marks frames of two or four words only.
    config.src_port_window_size = 1;
    // ADMAC's software realignment also needs the three-word frame size.
    let mut frame_bytes = 3u32 * 4;
    config.peripheral_config = ptr::from_mut(&mut frame_bytes).cast();
    config.peripheral_size = core::mem::size_of::<u32>();
    // SAFETY: `chan` is the live channel behind the substream.
    unsafe { super::dmaengine_slave_config(chan, &mut config) }
}

/// # Safety
///
/// Called by ALSA with a live substream of the microphone card.
unsafe extern "C" fn hpai_prepare(substream: *mut bindings::snd_pcm_substream) -> i32 {
    // SAFETY: by the caller's contract.
    unsafe { hpai_data(substream) }
        .hpai_stop()
        .map_or_else(|e| e.to_errno(), |()| 0)
}

/// # Safety
///
/// Called by ALSA, with the stream lock held, on a live substream of the
/// microphone card.
unsafe extern "C" fn hpai_trigger(substream: *mut bindings::snd_pcm_substream, cmd: i32) -> i32 {
    // SAFETY: by the caller's contract.
    let data = unsafe { hpai_data(substream) };
    // SAFETY: the dmaengine runtime was set up in `hpai_open`.
    let ret = unsafe { bindings::snd_dmaengine_pcm_trigger(substream, cmd) };
    if ret < 0 {
        return ret;
    }
    if cmd as u32 == bindings::SNDRV_PCM_TRIGGER_START {
        data.hpai_want_run.store(true, Relaxed);
        // Already queued: the pending start sees the flag.
        let _ = workqueue::system().enqueue(Arc::from(data));
    } else {
        data.hpai_want_run.store(false, Relaxed);
    }
    0
}

static HPAI_OPS: bindings::snd_pcm_ops = bindings::snd_pcm_ops {
    open: Some(hpai_open),
    close: Some(hpai_close),
    prepare: Some(hpai_prepare),
    trigger: Some(hpai_trigger),
    pointer: Some(bindings::snd_dmaengine_pcm_pointer),
    ioctl: None,
    hw_params: Some(hpai_hw_params),
    hw_free: None,
    sync_stop: Some(bindings::snd_dmaengine_pcm_sync_stop),
    get_time_info: None,
    fill_silence: None,
    copy: None,
    page: None,
    mmap: None,
    ack: None,
};

/// The microphone card, and the speaker and jack components where the audio
/// node lists their DMA channels.
pub(super) fn probe(pdev: &platform::Device<kernel::device::Core<'_>>) -> Result<SndSocAopDriver> {
    let data = SndSocAopData::probe(pdev, &HW_CFG_T8142)?;
    data.audio_attach_device(AUDIO_DEV_HPAI)?;
    let mut this = SndSocAopDriver::new(data.clone(), &HPAI_OPS)?;
    this.spkr = Some(aop_spkr::SpeakerComponent::register(data)?);
    Ok(this)
}
