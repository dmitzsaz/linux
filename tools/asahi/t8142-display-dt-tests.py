#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Check the compiled J704/J813 display handoff and connector graph.

Usage: python3 tools/asahi/t8142-display-dt-tests.py t8142-j704.dtb t8142-j813.dtb
Requires fdtget (from dtc). Static disabled controllers are intentional: aura
must reserve the running firmware, all live IOVAs and even empty frontend page
tables before publishing them. This checks topology, not hardware operation.
"""
import argparse
from pathlib import Path
import subprocess


class Tree:
    def __init__(self, path):
        self.path = str(path)
        self.phandles = {}
        self.walk("/")

    def get(self, node, prop, kind="x"):
        return subprocess.check_output(
            ["fdtget", "-t", kind, self.path, node, prop], text=True
        ).strip().split()

    def cells(self, node, prop):
        return [int(v, 16) for v in self.get(node, prop)]

    def walk(self, node):
        props = subprocess.check_output(
            ["fdtget", "-p", self.path, node], text=True
        ).splitlines()
        if "phandle" in props:
            self.phandles[self.cells(node, "phandle")[0]] = node
        for child in subprocess.check_output(
            ["fdtget", "-l", self.path, node], text=True
        ).splitlines():
            self.walk(node.rstrip("/") + "/" + child)

    def alias(self, name):
        return self.get("/aliases", name, "s")[0]

    def ref(self, node, prop):
        return self.phandles[self.cells(node, prop)[0]]

    def status(self, node, expected):
        assert self.get(node, "status", "s") == [expected], node

    def linked(self, endpoint):
        other = self.ref(endpoint, "remote-endpoint")
        assert self.ref(other, "remote-endpoint") == endpoint, endpoint
        return other


def check(path, board, phys):
    t = Tree(path)
    dcp = t.alias("dcpext1")
    assert dcp == "/soc/dcp@3aae00000"
    assert t.get(dcp, "compatible", "s") == ["apple,t8142-dcpext", "apple,dcpext"]
    t.status(dcp, "disabled")
    master = t.alias("disp0")
    assert t.alias("dispext1") == master
    scanout = t.cells(master, "iommus")
    assert len(scanout) == 4 and scanout[1::2] == [0, 0]
    assert t.phandles[scanout[0]] == "/soc/iommu@286300000"
    assert t.phandles[scanout[2]] == "/soc/iommu@3aa300000"
    piodma = t.alias("dispext1_piodma")
    assert piodma == dcp + "/piodma"
    assert t.cells(piodma, "iommus") == [scanout[2], 16]
    assert t.cells(dcp, "iommus")[1:] == [23]
    for provider in (t.ref(dcp, "iommus"), t.phandles[scanout[2]]):
        t.status(provider, "disabled")
        assert t.cells(provider, "apple,dma-range") == [0x100, 0, 0x10, 0]
        assert t.ref(provider, "power-domains") == t.ref(dcp, "power-domains")
    mailbox = t.ref(dcp, "mboxes")
    t.status(mailbox, "disabled")
    assert t.cells(mailbox, "interrupts")[1::3] == [925, 926, 927, 928]
    assert t.ref(mailbox, "power-domains") == t.ref(dcp, "power-domains")
    domain = t.ref(dcp, "power-domains")
    for offset in (0x3b8, 0x3a8, 0x2b8):
        t.status(domain, "disabled")
        assert t.cells(domain, "reg") == [offset, 4]
        assert t.get(domain, "apple,firmware-owned") == []
        if offset != 0x2b8:
            domain = t.ref(domain, "power-domains")
    for n in range(4):
        phy = f"/soc/phy@{0x403000000 + n * 0x8000000:x}"
        assert "lioa" in t.get(phy, "reg-names", "s"), phy
    assert t.get(dcp, "apple,connector-type", "s") == ["HDMI-A" if board == "j704" else "USB-C"]
    assert t.cells(dcp, "apple,typec-dptx-phys") == phys
    assert t.cells(dcp, "apple,typec-mux-indices") == [4] * len(phys)
    names = [f"typec{n}" for n in range(len(phys))]
    assert t.get(dcp, "phy-names", "s") == names + (["dp-phy"] if board == "j704" else [])
    assert t.get(dcp, "mux-control-names", "s") == names + (["dp-xbar"] if board == "j704" else [])
    muxes = t.cells(dcp, "mux-controls")
    phy_refs = t.cells(dcp, "phys")
    for route, n in enumerate(phys):
        xbar = t.phandles[muxes[2 * route]]
        assert xbar == f"/soc/mux-controller@{0x40304c000 + n * 0x8000000:x}"
        assert t.get(xbar, "compatible", "s") == ["apple,t8142-display-crossbar"]
        t.status(xbar, "okay")
        assert phy_refs[2 * route:2 * route + 2] == [
            t.cells(f"/soc/phy@{0x403000000 + n * 0x8000000:x}", "phandle")[0], 6
        ]
        endpoint = f"{dcp}/typec-routes/route@{route}/port/endpoint"
        connector = t.linked(endpoint)
        assert connector.endswith("/connector/ports/port@3/endpoint")
        superspeed = connector.split("/ports/")[0] + "/ports/port@1/endpoint"
        assert t.linked(superspeed).startswith(f"/soc/phy@{0x403000000 + n * 0x8000000:x}/")
        t.status(f"{dcp}/typec-routes/route@{route}", "disabled")
    if board == "j704":
        assert t.cells(dcp, "apple,dptx-phy") == [2]
        assert t.cells(dcp, "mux-index") == [4]
        assert t.cells(dcp, "hdmi-hpd-gpios")[1:] == [41, 0]
        assert t.cells(dcp, "hdmi-pwren-gpios")[1:] == [15, 0]
        assert t.cells(dcp, "dp2hdmi-pwren-gpios")[1:] == [26, 0]
        assert t.linked(dcp + "/ports/port@1/endpoint").startswith("/soc/phy@413000000/")
        assert t.phandles[muxes[-2]] == "/soc/mux-controller@41304c000"
        assert t.get("/soc/phy@413000000", "reg-names", "s") == ["core", "lpdptx", "axi2af", "lioa"]
        report = t.ref("/soc/phy@413000000", "power-domains")
        assert t.ref("/soc/mux-controller@41304c000", "power-domains") == report
        assert t.cells(report, "reg") == [26]
        assert "apple,t8142-pmp-v2-report-entry" in t.get(report, "compatible", "s")
    # Preserve the measured audio path without admitting an unsupported ABI.
    audio = t.linked(dcp + "/ports/port@0/endpoint").split("/ports/")[0]
    t.status(audio, "disabled")
    dma = t.ref(audio, "dmas")
    t.status(dma, "disabled")
    assert dma == "/soc/dma-controller@3aa140000"
    assert t.cells(audio, "dmas")[1:] == [0]
    assert t.cells(dma, "dma-channels") == [1]
    assert t.cells(dma, "iommus")[1:] == [30]
    t.status(t.ref(dma, "iommus"), "disabled")
    print(f"PASS {board}: DCPEXT1, retained suppliers, crossbars, connectors and audio topology")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("j704", type=Path, nargs="?")
    parser.add_argument("j813", type=Path, nargs="?")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--dtc", default="dtc")
    args = parser.parse_args()
    if args.output:
        root = Path(__file__).resolve().parents[2]
        args.output.mkdir(parents=True, exist_ok=True)
        for board in ("j704", "j813"):
            source = root / f"arch/arm64/boot/dts/apple/t8142-{board}.dts"
            preprocessed = args.output / f"t8142-{board}.dts"
            dtb = args.output / f"t8142-{board}.dtb"
            with preprocessed.open("w") as out:
                subprocess.run([
                    "cc", "-E", "-nostdinc", "-undef", "-D__DTS__", "-x", "assembler-with-cpp",
                    "-I", str(root / "scripts/dtc/include-prefixes"),
                    "-I", str(source.parent), str(source),
                ], stdout=out, check=True)
            subprocess.run([args.dtc, "-q", "-I", "dts", "-O", "dtb", "-o", str(dtb),
                            str(preprocessed)], check=True)
            setattr(args, board, dtb)
    if not args.j704 or not args.j813:
        parser.error("supply both DTBs or --output to compile them")
    check(args.j704, "j704", [3, 0, 1])
    check(args.j813, "j813", [0, 1])
