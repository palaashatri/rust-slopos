//! Which GPU adapter the wgpu executor uses, as a pure policy.
//!
//! [`WgpuExecutor::new_headless`](super::gpu::WgpuExecutor::new_headless)
//! lists the Vulkan adapters, turns each into a [`Candidate`] and asks
//! [`select_nvidia_vulkan`] for an NVIDIA one. The candidates are plain values
//! so the policy is tested here without a GPU or a driver. This module has no
//! wgpu types and is compiled in every build.
//!
//! Only the choice of an adapter is decided here. `SLOPOS_GPU` still decides
//! whether a window paints on the GPU at all (`platform::paint_path`), so
//! finding an NVIDIA adapter does not make the GPU path the default.

/// The PCI vendor ID of NVIDIA (0x10de). wgpu reports the same number in
/// `AdapterInfo::vendor`, and the Linux PCI sysfs `vendor` file contains it.
pub const NVIDIA_VENDOR: u32 = 0x10de;

/// The kind of device, as wgpu reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Discrete,
    Integrated,
    Virtual,
    /// A software device (llvmpipe and similar). Never chosen for NVIDIA.
    Cpu,
    Other,
}

/// The API a candidate adapter uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Vulkan,
    Gl,
    Other,
}

/// One adapter, reduced to the fields the policy reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub vendor: u32,
    pub kind: Kind,
    pub backend: Backend,
}

/// Index of the adapter to use on NVIDIA hardware.
///
/// It is an NVIDIA adapter on the Vulkan backend that is not a CPU device.
/// That covers the proprietary driver and NVK (Mesa's Vulkan driver for
/// nouveau). A discrete adapter is preferred to an integrated one, and an
/// integrated one to a virtual one. On a tie the first listed adapter wins.
///
/// `None` means there is no such adapter. The caller then uses its generic
/// choice, which is the behaviour without NVIDIA support.
pub fn select_nvidia_vulkan(candidates: &[Candidate]) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.vendor == NVIDIA_VENDOR && candidate.backend == Backend::Vulkan
        })
        .filter_map(|(index, candidate)| rank(candidate.kind).map(|rank| (rank, index)))
        .min()
        .map(|(_, index)| index)
}

/// Lower ranks are preferred. A CPU device has no rank and is never chosen.
fn rank(kind: Kind) -> Option<u8> {
    match kind {
        Kind::Discrete => Some(0),
        Kind::Integrated => Some(1),
        Kind::Virtual => Some(2),
        Kind::Other => Some(3),
        Kind::Cpu => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTEL: u32 = 0x8086;
    const MESA_SOFTWARE: u32 = 0x10005;

    fn candidate(vendor: u32, kind: Kind, backend: Backend) -> Candidate {
        Candidate {
            vendor,
            kind,
            backend,
        }
    }

    #[test]
    fn nvidia_vendor_is_the_pci_id() {
        assert_eq!(NVIDIA_VENDOR, 0x10de);
    }

    #[test]
    fn an_empty_list_selects_nothing() {
        assert_eq!(select_nvidia_vulkan(&[]), None);
    }

    #[test]
    fn a_discrete_nvidia_vulkan_adapter_is_selected_over_other_vendors() {
        let list = [
            candidate(INTEL, Kind::Integrated, Backend::Vulkan),
            candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Vulkan),
            candidate(MESA_SOFTWARE, Kind::Cpu, Backend::Vulkan),
        ];
        assert_eq!(select_nvidia_vulkan(&list), Some(1));
    }

    #[test]
    fn the_nvidia_adapter_is_found_when_it_is_listed_last() {
        let list = [
            candidate(INTEL, Kind::Integrated, Backend::Vulkan),
            candidate(MESA_SOFTWARE, Kind::Cpu, Backend::Vulkan),
            candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Vulkan),
        ];
        assert_eq!(select_nvidia_vulkan(&list), Some(2));
    }

    #[test]
    fn no_nvidia_adapter_selects_nothing_so_the_generic_choice_stays() {
        let list = [
            candidate(INTEL, Kind::Integrated, Backend::Vulkan),
            candidate(MESA_SOFTWARE, Kind::Cpu, Backend::Vulkan),
        ];
        assert_eq!(select_nvidia_vulkan(&list), None);
    }

    #[test]
    fn an_nvidia_adapter_on_the_gl_backend_is_not_selected() {
        let list = [
            candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Gl),
            candidate(INTEL, Kind::Integrated, Backend::Vulkan),
        ];
        assert_eq!(select_nvidia_vulkan(&list), None);
    }

    #[test]
    fn an_nvidia_software_device_is_not_selected() {
        let list = [candidate(NVIDIA_VENDOR, Kind::Cpu, Backend::Vulkan)];
        assert_eq!(select_nvidia_vulkan(&list), None);
    }

    #[test]
    fn a_discrete_adapter_beats_an_integrated_or_virtual_one() {
        let list = [
            candidate(NVIDIA_VENDOR, Kind::Virtual, Backend::Vulkan),
            candidate(NVIDIA_VENDOR, Kind::Integrated, Backend::Vulkan),
            candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Vulkan),
        ];
        assert_eq!(select_nvidia_vulkan(&list), Some(2));
    }

    #[test]
    fn on_a_tie_the_first_listed_nvidia_adapter_wins() {
        let list = [
            candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Vulkan),
            candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Vulkan),
        ];
        assert_eq!(select_nvidia_vulkan(&list), Some(0));
    }

    #[test]
    fn a_gl_nvidia_adapter_does_not_hide_a_vulkan_one() {
        let list = [
            candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Gl),
            candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Vulkan),
        ];
        assert_eq!(select_nvidia_vulkan(&list), Some(1));
    }

    #[test]
    fn an_unknown_backend_is_not_selected() {
        let list = [candidate(NVIDIA_VENDOR, Kind::Discrete, Backend::Other)];
        assert_eq!(select_nvidia_vulkan(&list), None);
    }
}
