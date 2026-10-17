//! AFH host channel classification from the Wi-Fi around (v1 afh.cpp)
//!
//! Bluetooth BR/EDR hops over 79 channels (2402-2480 MHz). With Adaptive
//! Frequency Hopping the central's controller leaves out channels it finds
//! bad; the host may help with HCI Set AFH Host Channel Classification,
//! marking channels it knows to be busy. A2DPWB is the central of its links,
//! so what it passes applies to headphones and receivers alike.
//!
//! Policies:
//!   "auto"   Wi-Fi access points this PC hears strongly: their 2.4 GHz
//!            channels are marked bad
//!   "off"    nothing is passed (the controller decides alone)
//!   "6,11"   these Wi-Fi channels are marked bad
//!
//! At least `MIN_CHANNELS` channels stay usable, as the specification
//! requires: channels of the weakest access points are dropped first.
//!
//! Plain functions over plain data: the access points come from
//! win-sys::wlan, converted by the caller into `WifiAp`.

/// Bluetooth BR/EDR channels
pub const CHANNELS: usize = 79;
/// The fewest channels the specification allows AFH to keep (Nmin = 20)
pub const MIN_CHANNELS: usize = 20;
/// Access points weaker than this are ignored by "auto"
pub const AUTO_RSSI_DBM: i32 = -75;
/// A 20 MHz Wi-Fi signal (22 MHz for 802.11b) spreads +-11 MHz around its
/// center
const WIFI_HALF_WIDTH_MHZ: i32 = 11;

/// One 2.4 GHz access point the PC hears
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiAp {
    /// 2.4 GHz channel number, 1-14
    pub channel: u8,
    pub rssi_dbm: i32,
}

/// What to pass to the controller
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    /// Bit n = Bluetooth channel n: 1 unknown (usable), 0 bad. Bit 79 is
    /// reserved and always 0.
    pub map: [u8; 10],
    /// Channels left usable
    pub usable: usize,
    /// Wi-Fi channels marked bad, in the order they were taken
    pub avoided_wifi: Vec<u8>,
    /// Something to pass to the controller (otherwise leave it alone)
    pub passed: bool,
}

impl Classification {
    /// Every channel usable, nothing to pass
    pub fn all_usable() -> Classification {
        Classification { map: all_usable_map(), usable: CHANNELS, avoided_wifi: Vec::new(), passed: false }
    }
}

/// A parsed policy string
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Policy {
    Off,
    Auto,
    /// Wi-Fi channels as written (out of range ones are skipped later)
    Manual(Vec<i64>),
}

impl Policy {
    /// "" and "off" are Off, "auto" is Auto, anything else is a channel
    /// list: every number in it counts, other characters only separate
    /// (as v1's strtol loop reads it, so "6, 11" and "6;11" both work)
    pub fn parse(policy: &str) -> Policy {
        match policy {
            "" | "off" => Policy::Off,
            "auto" => Policy::Auto,
            _ => Policy::Manual(parse_numbers(policy)),
        }
    }
}

/// Classification for a policy string. `survey` is called only for
/// "auto" (it scans nothing itself: win-sys::wlan returns what Windows'
/// background scans found).
pub fn classify(policy: &str, survey: impl FnOnce() -> Vec<WifiAp>) -> Classification {
    match Policy::parse(policy) {
        Policy::Off => Classification::all_usable(),
        Policy::Auto => avoid_wifi(&auto_channels(&survey())),
        Policy::Manual(channels) => avoid_wifi(&channels),
    }
}

/// The Wi-Fi channels "auto" avoids: those with an access point at or
/// above AUTO_RSSI_DBM, loudest first (by each channel's strongest access
/// point; ties by channel number)
pub fn auto_channels(aps: &[WifiAp]) -> Vec<i64> {
    let mut loudest: Vec<(u8, i32)> = Vec::new();
    for ap in aps.iter().filter(|ap| ap.rssi_dbm >= AUTO_RSSI_DBM) {
        match loudest.iter_mut().find(|(ch, _)| *ch == ap.channel) {
            Some((_, rssi)) => *rssi = (*rssi).max(ap.rssi_dbm),
            None => loudest.push((ap.channel, ap.rssi_dbm)),
        }
    }
    loudest.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    loudest.into_iter().map(|(ch, _)| i64::from(ch)).collect()
}

/// Classification that avoids these Wi-Fi channels in order, skipping
/// those out of range (1-14), repeated, or that would leave fewer than
/// MIN_CHANNELS usable
pub fn avoid_wifi(wifi_channels: &[i64]) -> Classification {
    let mut c = Classification::all_usable();
    for &ch in wifi_channels {
        let Ok(ch) = u8::try_from(ch) else { continue };
        if !(1..=14).contains(&ch) || c.avoided_wifi.contains(&ch) {
            continue;
        }
        let mut trial = c.map;
        mark_bad(&mut trial, ch);
        if count(&trial) < MIN_CHANNELS {
            continue; // would leave too few
        }
        c.map = trial;
        c.avoided_wifi.push(ch);
    }
    c.usable = count(&c.map);
    c.passed = !c.avoided_wifi.is_empty();
    c
}

/// Channels set in a 79-bit map (bit 79 is not counted)
pub fn count(map: &[u8; 10]) -> usize {
    (0..CHANNELS).filter(|&n| map[n / 8] & (1 << (n % 8)) != 0).count()
}

/// "6, 11"
pub fn describe(wifi_channels: &[u8]) -> String {
    wifi_channels.iter().map(u8::to_string).collect::<Vec<_>>().join(", ")
}

/// Center frequency of a 2.4 GHz Wi-Fi channel
pub fn wifi_center_mhz(channel: u8) -> i32 {
    if channel == 14 { 2484 } else { 2407 + 5 * i32::from(channel) }
}

fn all_usable_map() -> [u8; 10] {
    let mut map = [0xFF; 10];
    map[9] = 0x7F; // bit 79 is reserved
    map
}

/// Clears the Bluetooth channels within the Wi-Fi channel's width
fn mark_bad(map: &mut [u8; 10], wifi_channel: u8) {
    let center = wifi_center_mhz(wifi_channel);
    for n in 0..CHANNELS {
        let f = 2402 + n as i32;
        if (f - center).abs() <= WIFI_HALF_WIDTH_MHZ {
            map[n / 8] &= !(1 << (n % 8));
        }
    }
}

/// Every integer in `s`, as C's strtol loop reads them: optional leading
/// whitespace and sign, then digits; any other character is skipped
fn parse_numbers(s: &str) -> Vec<i64> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < b.len() {
        let mut q = p;
        while q < b.len() && b[q].is_ascii_whitespace() {
            q += 1;
        }
        let negative = q < b.len() && b[q] == b'-';
        if q < b.len() && (b[q] == b'-' || b[q] == b'+') {
            q += 1;
        }
        let digits_start = q;
        let mut value: i64 = 0;
        while q < b.len() && b[q].is_ascii_digit() {
            value = value.saturating_mul(10).saturating_add(i64::from(b[q] - b'0'));
            q += 1;
        }
        if q == digits_start {
            p += 1; // no number here
            continue;
        }
        out.push(if negative { -value } else { value });
        p = q;
    }
    out
}
