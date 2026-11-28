//! Codec discovery: which decoders the installed GStreamer provides, and
//! which package to install for the ones it lacks.
//!
//! The table below only says which decoder element handles which codec and
//! which package usually ships it. Nothing is claimed as available from the
//! table: a codec counts as available only when [`discover`] finds one of its
//! decoder elements in the GStreamer registry.

use gstreamer as gst;

/// A codec SLOPOS may meet in a file, and how to get a decoder for it.
#[derive(Debug, PartialEq, Eq)]
pub struct Codec {
    /// The name shown to users.
    pub label: &'static str,
    /// Lowercase words that appear in GStreamer's caps descriptions and error
    /// text for this codec. Each must resolve back to this entry.
    pub names: &'static [&'static str],
    /// Decoder elements that handle the codec; any one of them is enough.
    pub decoders: &'static [&'static str],
    /// The distribution package that usually provides a decoder.
    pub package: &'static str,
}

const LIBAV: &str = "gstreamer1.0-libav (FFmpeg decoders; check codec licensing in your country)";
const GOOD: &str = "gstreamer1.0-plugins-good";
const BASE: &str = "gstreamer1.0-plugins-base";
const BAD: &str = "gstreamer1.0-plugins-bad";
/// Used when a missing decoder matches no entry.
pub const DEFAULT_PACKAGE: &str = "gstreamer1.0-plugins-good or gstreamer1.0-plugins-bad";

/// The codecs SLOPOS names in its messages. Patent-encumbered codecs are
/// not shipped; their decoders come from the distribution's libav package.
pub const CODECS: &[Codec] = &[
    Codec {
        label: "H.264 video",
        names: &["h.264", "h264", "avc"],
        decoders: &["avdec_h264", "openh264dec"],
        package: LIBAV,
    },
    Codec {
        label: "H.265 video",
        names: &["h.265", "h265", "hevc"],
        decoders: &["avdec_h265"],
        package: LIBAV,
    },
    Codec {
        label: "MPEG-4 video",
        names: &["mpeg-4 video", "mpeg4 video"],
        decoders: &["avdec_mpeg4"],
        package: LIBAV,
    },
    Codec {
        label: "AAC audio",
        names: &["aac"],
        decoders: &["avdec_aac", "faad"],
        package: LIBAV,
    },
    Codec {
        label: "Windows Media",
        names: &["wmv", "wma"],
        decoders: &["avdec_wmv3", "avdec_wmav2"],
        package: LIBAV,
    },
    Codec {
        label: "MP3 audio",
        names: &["mp3", "mpeg-1 layer 3", "mpeg audio"],
        decoders: &["mpg123audiodec", "avdec_mp3"],
        package: "gstreamer1.0-plugins-ugly or gstreamer1.0-libav (MP3 decoding)",
    },
    Codec {
        label: "AC-3 audio",
        names: &["ac-3", "ac3", "a/52"],
        decoders: &["a52dec", "avdec_ac3"],
        package: "gstreamer1.0-plugins-ugly or gstreamer1.0-libav",
    },
    Codec {
        label: "AV1 video",
        names: &["av1", "dav1d"],
        decoders: &["dav1ddec", "av1dec"],
        package: BAD,
    },
    Codec {
        label: "VP8 video",
        names: &["vp8"],
        decoders: &["vp8dec"],
        package: GOOD,
    },
    Codec {
        label: "VP9 video",
        names: &["vp9"],
        decoders: &["vp9dec"],
        package: GOOD,
    },
    Codec {
        label: "Theora video",
        names: &["theora"],
        decoders: &["theoradec"],
        package: BASE,
    },
    Codec {
        label: "Vorbis audio",
        names: &["vorbis"],
        decoders: &["vorbisdec"],
        package: BASE,
    },
    Codec {
        label: "Opus audio",
        names: &["opus"],
        decoders: &["opusdec"],
        package: BASE,
    },
    Codec {
        label: "FLAC audio",
        names: &["flac"],
        decoders: &["flacdec"],
        package: GOOD,
    },
];

/// The codec named in a caps description or error text, if any. Entries are
/// tried in table order, so the first match wins.
pub fn lookup(text: &str) -> Option<&'static Codec> {
    let lower = text.to_lowercase();
    CODECS
        .iter()
        .find(|codec| codec.names.iter().any(|name| lower.contains(name)))
}

/// Which package to install for a codec named in `text`.
pub fn install_hint(text: &str) -> &'static str {
    lookup(text).map_or(DEFAULT_PACKAGE, |codec| codec.package)
}

/// The first decoder element of `codec` that `has_element` reports, or `None`.
/// The app passes the GStreamer registry lookup; tests pass a fake registry.
/// A codec is never reported as available from the table alone.
pub fn probe(codec: &Codec, has_element: impl Fn(&str) -> bool) -> Option<&'static str> {
    codec
        .decoders
        .iter()
        .copied()
        .find(|decoder| has_element(decoder))
}

/// The codec named in `text` when the probe finds none of its decoders.
/// A codec whose decoder is installed is not missing: the failure is
/// something else, and the caller must not blame the codec.
pub fn missing_decoder_in(
    text: &str,
    has_element: impl Fn(&str) -> bool,
) -> Option<&'static Codec> {
    let codec = lookup(text)?;
    probe(codec, has_element).is_none().then_some(codec)
}

/// The GStreamer registry of this process. GStreamer must be initialised
/// first (see [`crate::engine::init`]).
fn registry_has(name: &str) -> bool {
    gst::ElementFactory::find(name).is_some()
}

/// [`missing_decoder_in`] against the installed registry.
pub fn missing_decoder(text: &str) -> Option<&'static Codec> {
    missing_decoder_in(text, registry_has)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::backend_or_not_run;
    use std::collections::HashSet;

    #[test]
    fn every_name_resolves_to_its_own_codec() {
        for codec in CODECS {
            assert!(!codec.decoders.is_empty(), "{} has no decoder", codec.label);
            assert!(!codec.package.is_empty(), "{} has no package", codec.label);
            for name in codec.names {
                assert_eq!(*name, name.to_lowercase(), "names are matched lowercase");
                assert_eq!(lookup(name), Some(codec), "{name} resolves elsewhere");
            }
        }
        let mut labels = HashSet::new();
        assert!(CODECS.iter().all(|codec| labels.insert(codec.label)));
    }

    #[test]
    fn names_are_found_inside_gstreamer_text() {
        assert_eq!(
            lookup("H.264 (Main Profile) decoder").map(|c| c.label),
            Some("H.264 video")
        );
        assert_eq!(lookup("AV1 decoder").map(|c| c.label), Some("AV1 video"));
        assert_eq!(lookup("something else entirely"), None);
        assert!(install_hint("H.264 (Main Profile) decoder").contains("libav"));
        assert!(install_hint("AV1 decoder").contains("bad"));
        assert_eq!(install_hint("unknown"), DEFAULT_PACKAGE);
    }

    /// The codecs the probe finds in a fake registry that holds `present`.
    fn found(present: &[&str]) -> Vec<(&'static str, &'static str)> {
        CODECS
            .iter()
            .filter_map(|codec| {
                probe(codec, |name| present.contains(&name)).map(|d| (codec.label, d))
            })
            .collect()
    }

    #[test]
    fn the_probe_reports_only_decoders_that_are_present() {
        assert_eq!(
            found(&["vp8dec", "opusdec"]),
            [("VP8 video", "vp8dec"), ("Opus audio", "opusdec")]
        );
        // An empty registry claims nothing, whatever the table lists.
        assert_eq!(found(&[]), []);
    }

    #[test]
    fn a_codec_with_a_fallback_decoder_is_available_through_it() {
        let aac = CODECS.iter().find(|c| c.label == "AAC audio").unwrap();
        assert_eq!(probe(aac, |name| name == "faad"), Some("faad"));
        assert_eq!(probe(aac, |_| false), None);
    }

    #[test]
    fn a_missing_decoder_is_named_only_when_no_decoder_is_installed() {
        let none = |_: &str| false;
        assert_eq!(
            missing_decoder_in("H.264 (Main Profile) decoder", none).map(|c| c.label),
            Some("H.264 video")
        );
        // An installed decoder means the failure is something else.
        assert_eq!(
            missing_decoder_in("H.264 (Main Profile) decoder", |n| n == "avdec_h264"),
            None
        );
        assert_eq!(missing_decoder_in("VP8 video", |n| n == "vp8dec"), None);
        assert_eq!(
            missing_decoder_in("VP8 video", none).map(|c| c.label),
            Some("VP8 video")
        );
        assert_eq!(missing_decoder_in("nothing known", none), None);
    }

    #[test]
    fn the_installed_registry_is_probed() {
        if !backend_or_not_run("the_installed_registry_is_probed") {
            return;
        }
        crate::engine::init().unwrap();
        // Each answer is the registry's own: a codec is available exactly
        // when one of its decoders is registered.
        let h264 = CODECS.iter().find(|c| c.label == "H.264 video").unwrap();
        let h264_installed = registry_has("avdec_h264") || registry_has("openh264dec");
        assert_eq!(probe(h264, registry_has).is_some(), h264_installed);
        assert_eq!(
            missing_decoder("H.264 (Main Profile) decoder").is_some(),
            !h264_installed
        );
        // The base and good plugins the fixtures use are installed here and
        // in CI; VP8 must be reported through its decoder.
        let vp8 = CODECS.iter().find(|c| c.label == "VP8 video").unwrap();
        if registry_has("vp8dec") {
            assert_eq!(probe(vp8, registry_has), Some("vp8dec"));
            assert_eq!(missing_decoder("VP8 video"), None);
        }
    }
}
