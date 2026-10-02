#!/usr/bin/env python3
"""Packaging/ELF metadata fixtures; these do not execute Linux or establish C conformance."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

CAPI = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(CAPI))
import collect_distribution as collector
import distribution
from distribution_platform import TARGETS, build_environment, elf_metadata, glibc_floor, native_target


HEADER = """ELF Header:
  Class:                             ELF64
  Data:                              2's complement, little endian
  Type:                              DYN (Shared object file)
  Machine:                           Advanced Micro Devices X86-64
"""
DYNAMIC = """ 0x000000000000000e (SONAME)             Library soname: [libucl.so.1]
 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]
 0x0000000000000001 (NEEDED)             Shared library: [libgcc_s.so.1]
"""
VERSIONS = "Name: GLIBC_2.9\nName: GLIBC_2.2.5\nName: GLIBC_2.34\n"


class PlatformTests(unittest.TestCase):
    def test_native_matrix_and_cross_host_rejection(self):
        for target in TARGETS.values():
            self.assertEqual(native_target(target.name, system=target.system, machine=target.machine), target)
        with self.assertRaises(RuntimeError):
            native_target("linux-arm64", system="Linux", machine="x86_64")
        with self.assertRaises(RuntimeError):
            native_target("linux-amd64", system="Darwin", machine="arm64")

    def test_deployment_environment_is_platform_specific(self):
        self.assertEqual(build_environment(TARGETS["macos-arm64"])["MACOSX_DEPLOYMENT_TARGET"], "11.0")
        self.assertNotIn("MACOSX_DEPLOYMENT_TARGET", build_environment(TARGETS["linux-amd64"]))

    def test_elf_machines_and_glibc_numeric_floor(self):
        self.assertEqual(glibc_floor(VERSIONS), "2.34")
        amd64 = elf_metadata(HEADER, DYNAMIC, VERSIONS, TARGETS["linux-amd64"])
        arm64 = elf_metadata(HEADER.replace("Advanced Micro Devices X86-64", "AArch64"),
                             DYNAMIC, VERSIONS, TARGETS["linux-arm64"])
        self.assertEqual(amd64["shared_glibc_symbol_floor"], "2.34")
        self.assertEqual(arm64["machine"], "AArch64")

    def test_malformed_architecture_soname_paths_and_glibc_reject(self):
        for header, dynamic, versions in [
                (HEADER, DYNAMIC.replace("libucl.so.1", "libucl.so.0"), VERSIONS),
                (HEADER.replace("ELF64", "ELF32"), DYNAMIC, VERSIONS),
                (HEADER.replace("X86-64", "AArch64"), DYNAMIC, VERSIONS),
                (HEADER.replace("little endian", "big endian"), DYNAMIC, VERSIONS),
                (HEADER, DYNAMIC + " (RUNPATH) Library runpath: [/build/lib]\n", VERSIONS),
                (HEADER, DYNAMIC.replace("libc.so.6", "/build/libc.so.6"), VERSIONS),
                (HEADER, DYNAMIC, "Name: GLIBC_PRIVATE\n"),
                (HEADER, DYNAMIC, "Name: GLIBC_ABI_DT_RELR\n"),
                (HEADER, DYNAMIC, "No version information found")]:
            with self.subTest(header=header, dynamic=dynamic, versions=versions):
                with self.assertRaises(RuntimeError):
                    elf_metadata(header, dynamic, versions, TARGETS["linux-amd64"])

    def test_linux_never_uses_darwin_snapshots_as_default(self):
        with self.assertRaisesRegex(RuntimeError, "actual native reference evidence"):
            distribution.golden_evidence(distribution.ROOT, TARGETS["linux-amd64"])


class CollectionTests(unittest.TestCase):
    # Synthetic metadata/bytes exercise collection gates only, never C execution.
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="c15-metadata-fixture-")
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.incoming = self.work / "incoming"
        self.incoming.mkdir()
        self.output = self.work / "output"
        self.commit = "a" * 40
        self.version = "0.6.0"
        self.payloads = {}
        inventory = json.loads((collector.ROOT / "docs/spec/c-api/stage-a/api.json").read_text())
        cases = json.loads((collector.ROOT / "docs/spec/c-api/stage-a/cases.json").read_text())["cases"]
        for kind in ("source", *TARGETS):
            info = {"commit": self.commit, "rust_package_version": self.version,
                    "abi_version": "1.0.0", "spec": "spec-v22", "cargo_c": distribution.CARGO_C,
                    "dirty": False, "verified": True, "kind": "source" if kind == "source" else "sdk",
                    "target": None if kind == "source" else TARGETS[kind].rust}
            files = {}
            if kind != "source":
                target = TARGETS[kind]
                info["native_host"] = {"system": target.system, "machine": target.machine, "glibc": "2.39"}
                info["deployment"] = ({"format": "ELF64", "machine": target.elf_machine,
                                       "soname": target.versioned, "shared_glibc_symbol_floor": "2.34",
                                       "static_consumer_glibc_symbol_floor": "2.38", "glibc_symbol_floor": "2.38"}
                                      if target.system == "Linux" else
                                      {"format": "Mach-O", "machine": "arm64", "minimum_macos": "11.0", "install_name": "@rpath/libucl.1.dylib"})
                evidence = {"reference_commit": inventory["reference_commit"],
                            "reference_header_sha256": inventory["reference_header_sha256"],
                            "platform": target.golden, "machine": target.machine, "system": target.system,
                            "provided_functions": 43,
                            "contract_sha256": collector.digest(collector.ROOT / "docs/spec/c-api/stage-a/api.json"),
                            "probe_sha256": collector.digest(collector.ROOT / "tests/conformance/capi/stage-a/probe.c"),
                            "header_library_combinations_per_candidate": 4, "cases": {}}
                for case in cases:
                    data = b"synthetic packaging fixture; not oracle output\n"
                    files[f"conformance/{case}.txt"] = data
                    evidence["cases"][case] = hashlib.sha256(data).hexdigest()
                info["conformance"] = evidence
                files["conformance/reference-evidence.json"] = json.dumps(evidence).encode()
            self.payloads[kind] = (info, files)
            self.write(kind)

    def write(self, kind):
        info, files = self.payloads[kind]
        name = f"serde-ucl-c-{self.version}-{kind}"
        archive = self.incoming / (name + ".tar.gz")
        with tarfile.open(archive, "w:gz") as tar:
            for path, data in {"BUILD-INFO.json": json.dumps(info).encode(), **files}.items():
                entry = tarfile.TarInfo(name + "/" + path)
                entry.size = len(data)
                tar.addfile(entry, io.BytesIO(data))
        (self.incoming / ("SHA256SUMS-" + kind)).write_text(f"{collector.digest(archive)}  {archive.name}\n")

    def collect(self):
        collector.collect(self.incoming, self.output, self.version, self.commit)

    def test_complete_collection_has_four_archives_one_checksum(self):
        self.collect()
        self.assertEqual(len(list(self.output.iterdir())), 5)
        self.assertEqual(len((self.output / "SHA256SUMS").read_text().splitlines()), 4)

    def test_missing_extra_and_corrupt_inputs_reject(self):
        checksum = self.incoming / "SHA256SUMS-source"
        original = checksum.read_text()
        checksum.unlink()
        with self.assertRaises(RuntimeError):
            self.collect()
        checksum.write_text(original)
        extra = self.incoming / "unexpected.tar.gz"
        extra.write_bytes(b"extra")
        with self.assertRaises(RuntimeError):
            self.collect()
        extra.unlink()
        checksum.write_text(original.replace(original[:64], "0" * 64))
        with self.assertRaises(RuntimeError):
            self.collect()

    def test_native_reference_identity_and_darwin_architecture_reject(self):
        for kind, section, key, invalid in [
                ("linux-arm64", "conformance", "machine", "x86_64"),
                ("linux-arm64", "conformance", "system", "Darwin"),
                ("macos-arm64", "deployment", "format", "ELF64"),
                ("macos-arm64", "deployment", "machine", "x86_64")]:
            info, files = self.payloads[kind]
            original = info[section][key]
            info[section][key] = invalid
            if section == "conformance":
                files["conformance/reference-evidence.json"] = json.dumps(info[section]).encode()
            self.write(kind)
            with self.assertRaises(RuntimeError):
                self.collect()
            info[section][key] = original
            if section == "conformance":
                files["conformance/reference-evidence.json"] = json.dumps(info[section]).encode()
            self.write(kind)

    def test_commit_target_deployment_and_snapshot_mismatches_reject(self):
        info, files = self.payloads["linux-arm64"]
        for key, value in [("commit", "b" * 40), ("target", TARGETS["linux-amd64"].rust),
                           ("dirty", True), ("verified", False)]:
            original = info[key]
            info[key] = value
            self.write("linux-arm64")
            with self.assertRaises(RuntimeError):
                self.collect()
            info[key] = original
        info["deployment"]["glibc_symbol_floor"] = "2.9"
        self.write("linux-arm64")
        with self.assertRaises(RuntimeError):
            self.collect()
        info["deployment"]["glibc_symbol_floor"] = "2.38"
        files["conformance/abi.txt"] = b"tampered structural fixture"
        self.write("linux-arm64")
        with self.assertRaises(RuntimeError):
            self.collect()


if __name__ == "__main__":
    unittest.main()
