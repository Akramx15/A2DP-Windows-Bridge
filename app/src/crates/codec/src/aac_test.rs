//! Switches for finding out why a sink stays silent with AAC (issue #87).
//!
//! `A2DPWB_AAC_TEST` holds comma-separated switches, read once per process;
//! unset (the default) changes nothing:
//!
//! - `marker`: RTP marker bit set on every AAC packet
//! - `cfgbr0`: the bit rate field of the AAC configuration is 0 (unknown)
//! - `br=N`: encoder bit rate N bit/s instead of the quality's
//! - `nopeak`: no AACENC_PEAK_BITRATE
//! - `noab`: no afterburner
//! - `dump`: log the first bytes of the first AAC packets

use std::sync::OnceLock;

#[derive(Debug, Default, Clone, Copy)]
pub struct AacTest {
    pub marker: bool,
    pub cfg_bitrate0: bool,
    pub bitrate: Option<u32>,
    pub no_peak: bool,
    pub no_afterburner: bool,
    pub dump: bool,
}

impl AacTest {
    pub fn parse(spec: &str) -> AacTest {
        let mut t = AacTest::default();
        for item in spec.split(',').map(str::trim) {
            match item {
                "marker" => t.marker = true,
                "cfgbr0" => t.cfg_bitrate0 = true,
                "nopeak" => t.no_peak = true,
                "noab" => t.no_afterburner = true,
                "dump" => t.dump = true,
                _ => {
                    if let Some(n) = item.strip_prefix("br=").and_then(|n| n.parse().ok()) {
                        t.bitrate = Some(n);
                    }
                }
            }
        }
        t
    }
}

/// The switches of this process (logged once when any is on)
pub fn aac_test() -> &'static AacTest {
    static TEST: OnceLock<AacTest> = OnceLock::new();
    TEST.get_or_init(|| {
        let spec = std::env::var("A2DPWB_AAC_TEST").unwrap_or_default();
        let t = AacTest::parse(&spec);
        if !spec.trim().is_empty() {
            tracing::warn!("AAC test switches on (A2DPWB_AAC_TEST): {t:?}");
        }
        t
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_switches() {
        let t = AacTest::parse("marker, br=128000,noab,bogus");
        assert!(t.marker && t.no_afterburner && !t.cfg_bitrate0 && !t.dump);
        assert_eq!(t.bitrate, Some(128_000));
        assert!(!AacTest::parse("").marker);
    }
}
