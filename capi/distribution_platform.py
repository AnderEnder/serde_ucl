"""Native SDK target identity and deployment metadata; no C behavior definitions."""
from dataclasses import dataclass
import os
from pathlib import Path
import platform
import re


@dataclass(frozen=True)
class Target:
    name: str
    system: str
    machine: str
    rust: str
    golden: str
    extension: str
    elf_machine: str | None = None

    @property
    def shared(self):
        return "libucl." + self.extension

    @property
    def versioned(self):
        return "libucl.1.dylib" if self.system == "Darwin" else "libucl.so.1"

    @property
    def full(self):
        return "libucl.1.0.0.dylib" if self.system == "Darwin" else "libucl.so.1.0.0"


TARGETS = {
    "macos-arm64": Target("macos-arm64", "Darwin", "arm64", "aarch64-apple-darwin",
                          "darwin-arm64", "dylib"),
    "linux-amd64": Target("linux-amd64", "Linux", "x86_64", "x86_64-unknown-linux-gnu",
                          "linux-x86_64", "so", "Advanced Micro Devices X86-64"),
    "linux-arm64": Target("linux-arm64", "Linux", "aarch64", "aarch64-unknown-linux-gnu",
                          "linux-aarch64", "so", "AArch64"),
}


def native_target(name=None, *, system=None, machine=None):
    system = system or platform.system()
    machine = machine or platform.machine()
    if name:
        target = TARGETS[name]
        if (system, machine) != (target.system, target.machine):
            raise RuntimeError(f"{name} requires native {target.system}/{target.machine}; "
                               f"actual host is {system}/{machine}")
        return target
    for target in TARGETS.values():
        if (system, machine) == (target.system, target.machine):
            return target
    raise RuntimeError(f"Unsupported native SDK host: {system}/{machine}")


def build_environment(target):
    env = dict(os.environ, LC_ALL="C", TZ="UTC")
    for name in ("CARGO_TARGET_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                 "MACOSX_DEPLOYMENT_TARGET", "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"):
        env.pop(name, None)
    if target.system == "Darwin":
        env["MACOSX_DEPLOYMENT_TARGET"] = "11.0"
    return env


def version_tuple(version):
    return tuple(map(int, version.split(".")))


def glibc_floor(version_info):
    names = set(re.findall(r"\bGLIBC_([A-Za-z0-9_.]+)", version_info))
    if not names:
        raise RuntimeError("No GLIBC version requirements found in Linux artifact")
    if any(not re.fullmatch(r"\d+(?:\.\d+)+", name) for name in names):
        raise RuntimeError(f"Unrecognized GLIBC ABI requirements: {sorted(names)}")
    return max(names, key=version_tuple)


def elf_metadata(header, dynamic, version_info, target):
    def field(name):
        match = re.search(r"^\s*" + name + r":\s*(.+)$", header, re.MULTILINE)
        return match.group(1).strip() if match else ""
    if field("Class") != "ELF64" or "little endian" not in field("Data"):
        raise RuntimeError("Linux SDK must be ELF64 little-endian")
    if field("Machine") != target.elf_machine or not field("Type").startswith("DYN"):
        raise RuntimeError(f"ELF target mismatch: {field('Machine')}/{field('Type')}")
    sonames = re.findall(r"\(SONAME\).*?\[([^]]+)\]", dynamic)
    if sonames != [target.versioned]:
        raise RuntimeError(f"Expected SONAME {target.versioned}; found {sonames}")
    if re.search(r"\((?:RPATH|RUNPATH)\)", dynamic):
        raise RuntimeError("SDK shared library must not carry a build/install RPATH or RUNPATH")
    needed = re.findall(r"\(NEEDED\).*?\[([^]]+)\]", dynamic)
    if not needed or any("/" in item for item in needed):
        raise RuntimeError(f"Invalid ELF dependency identity: {needed}")
    return {"format": "ELF64", "machine": target.elf_machine, "soname": target.versioned,
            "needed_libraries": needed, "shared_glibc_symbol_floor": glibc_floor(version_info)}


def inspect_library(prefix, target, run, *, portable):
    library = prefix / "lib" / target.versioned
    if target.system == "Linux":
        return elf_metadata(run(["readelf", "--file-header", "--wide", library]),
                            run(["readelf", "--dynamic", "--wide", library]),
                            run(["readelf", "--version-info", "--wide", library]), target)
    identity = run(["otool", "-L", library])
    expected = "@rpath/" + target.versioned if portable else str(library)
    if expected not in identity or "compatibility version 1.0.0, current version 1.0.0" not in identity:
        raise RuntimeError(f"Darwin ABI identity/version mismatch: {identity}")
    build = run(["vtool", "-show-build", library])
    if not re.search(r"minos\s+11\.0", build):
        raise RuntimeError(f"Darwin deployment floor mismatch: {build}")
    architecture = run(["lipo", "-archs", library]).strip()
    if architecture != "arm64":
        raise RuntimeError(f"SDK must contain only arm64, found {architecture}")
    return {"format": "Mach-O", "machine": "arm64", "install_name": expected,
            "minimum_macos": "11.0"}


def inspect_consumer(binary, target, run):
    if target.system != "Linux":
        return None
    return glibc_floor(run(["readelf", "--version-info", "--wide", binary]))


def host_metadata(target, run):
    metadata = {"system": platform.system(), "machine": platform.machine(),
                "kernel": platform.release()}
    if target.system == "Linux":
        glibc = run(["getconf", "GNU_LIBC_VERSION"]).strip()
        if not re.fullmatch(r"glibc \d+(?:\.\d+)+", glibc):
            raise RuntimeError(f"GNU glibc target required; found {glibc}")
        metadata["glibc"] = glibc.removeprefix("glibc ")
        os_release = Path("/etc/os-release")
        metadata["os_release"] = os_release.read_text() if os_release.is_file() else "unavailable"
    else:
        metadata["macos_version"] = run(["sw_vers", "-productVersion"]).strip()
    return metadata
