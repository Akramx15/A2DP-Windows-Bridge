//! The command line: `a2dpwb-core --cli -d XX:XX:XX:XX:XX:XX [options]`.
//! The options are v1's; streaming goes through the same core service as the
//! resident process.
//!
//! Without -d (and without a receiver giving its address), the first
//! Windows-paired device whose class of device is Audio/Video is used, as v1
//! did; -l lists the devices Windows has paired. Both read Windows' device
//! list only: the adapter A2DPWB uses is not touched.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use a2dpwb_core::{Service, ServiceEvent, ServiceOptions, SourceRequest, StreamRequest, StreamState, VolumeSync};
use bt_host::HciCapture;
use bt_host::media::clamp_media_payload_limit;
use codec::{CodecKind, Quality};
use hci_transport::UsbSelector;
use win_sys::bt_paired::{self, PairedDevice};

const USAGE: &str = "\
Usage: a2dpwb-core --cli [-d XX:XX:XX:XX:XX:XX] [options]

  -d <address>          Bluetooth address of the headphones / receiver
                        (default: the first audio device paired with Windows)
  -l                    list the Bluetooth devices paired with Windows and exit
  -u <adapter>          USB Bluetooth adapter to use when several have WinUSB:
                        instance ID (USB\\VID_0BDA&PID_8771\\...), device path
                        (\\\\?\\usb#vid_0bda&pid_8771#...) or VID:PID (0bda:8771)
  -c <codec>           ldac, aptxhd, aptxll, aptx, sbc, aac, lhdcv5, auto (default)
  -q <quality>          hq (default), sq, mq; mhq for LHDC V5
  -a                    adaptive bit rate (LDAC, LHDC V5)
  -m <mode>             capture: loopback (default), virtual, app
  --audio-device <id>   render device for -m virtual
  --app <exe>           executable for -m app
  --rate <hz>           capture sample rate (default: the device's)
  --bits <16|24>        bit depth (default: 24 where the codec carries it)
  --max-packet <bytes>  max media packet size (679-1679, default 1023)
  --afh <policy>        auto, off or Wi-Fi channels to avoid such as 6,11
  --remote-sink <host[:port]|auto>
                        statistics of a measuring receiver (tools/linux_sink);
                        without -d, stream to that receiver
  --scan [seconds]      list nearby devices and exit

Development / testing:
  --hci-tcp <host:port> H4 over TCP to a virtual controller (tools/emu)
  --hci-capture <path>  HCI capture (.pklg) of the whole run
  --test-tone           1 kHz left / 1.5 kHz right at 48 kHz instead of capture
  --duration <seconds>  stop after this time (default: Ctrl+C)
";

struct Args {
    device: Option<String>,
    /// -l: list the Windows-paired devices and exit
    list: bool,
    /// -u: the adapter, as given and as parsed
    usb: Option<(String, UsbSelector)>,
    codec: Option<CodecKind>,
    quality: Quality,
    abr: bool,
    mode: String,
    audio_device: String,
    app: String,
    rate: Option<u32>,
    bits: Option<u8>,
    max_packet: u16,
    afh: Option<String>,
    remote_sink: Option<String>,
    scan: Option<u32>,
    hci_tcp: Option<String>,
    hci_capture: Option<PathBuf>,
    test_tone: bool,
    duration: Option<u64>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args {
        device: None,
        list: false,
        usb: None,
        codec: None,
        quality: Quality::High,
        abr: false,
        mode: "loopback".into(),
        audio_device: String::new(),
        app: String::new(),
        rate: None,
        bits: None,
        max_packet: bt_host::media::MEDIA_PAYLOAD_LIMIT_DEFAULT,
        afh: None,
        remote_sink: None,
        scan: None,
        hci_tcp: None,
        hci_capture: None,
        test_tone: false,
        duration: None,
    };
    let mut it = args.iter().peekable();
    let value = |it: &mut std::iter::Peekable<std::slice::Iter<String>>, name: &str| {
        it.next().cloned().ok_or_else(|| format!("{name} needs a value"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--cli" => {}
            "-d" => a.device = Some(value(&mut it, "-d")?),
            "-l" => a.list = true,
            "-u" => {
                let u = value(&mut it, "-u")?;
                let selector = UsbSelector::from_arg(&u)?;
                a.usb = Some((u.trim().to_string(), selector));
            }
            "-c" => {
                let c = value(&mut it, "-c")?;
                a.codec = if c.eq_ignore_ascii_case("auto") {
                    None
                } else {
                    Some(CodecKind::from_key(&c).ok_or_else(|| {
                        format!("Unknown codec '{c}'. Use: ldac, aptxhd, aptxll, aptx, sbc, aac, lhdcv5, auto")
                    })?)
                };
            }
            "-q" => a.quality = Quality::from_key(&value(&mut it, "-q")?.to_lowercase()),
            "-a" => a.abr = true,
            "-m" => a.mode = value(&mut it, "-m")?.to_lowercase(),
            "--audio-device" => a.audio_device = value(&mut it, "--audio-device")?,
            "--app" => a.app = value(&mut it, "--app")?,
            "--rate" => a.rate = Some(value(&mut it, "--rate")?.parse().map_err(|_| "bad --rate")?),
            "--bits" => a.bits = Some(value(&mut it, "--bits")?.parse().map_err(|_| "bad --bits")?),
            "--max-packet" => {
                let v: u32 = value(&mut it, "--max-packet")?.parse().map_err(|_| "bad --max-packet")?;
                a.max_packet = clamp_media_payload_limit(v);
                if a.max_packet as u32 != v {
                    println!("--max-packet {v} is outside 679-1679, using {}", a.max_packet);
                }
            }
            "--afh" => a.afh = Some(value(&mut it, "--afh")?),
            "--remote-sink" => a.remote_sink = Some(value(&mut it, "--remote-sink")?),
            "--scan" => {
                let secs = match it.peek() {
                    Some(s) if !s.starts_with('-') => it.next().and_then(|s| s.parse().ok()).unwrap_or(10),
                    _ => 10,
                };
                a.scan = Some(secs);
            }
            "--hci-tcp" => a.hci_tcp = Some(value(&mut it, "--hci-tcp")?),
            "--hci-capture" => a.hci_capture = Some(value(&mut it, "--hci-capture")?.into()),
            "--test-tone" => a.test_tone = true,
            "--duration" => a.duration = Some(value(&mut it, "--duration")?.parse().map_err(|_| "bad --duration")?),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(a)
}

pub fn run(args: &[String]) -> i32 {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}\n");
            }
            print!("{USAGE}");
            return if e.is_empty() { 0 } else { 2 };
        }
    };
    if a.list {
        // Windows' list only: no service, no adapter
        return match bt_paired::paired_devices() {
            Ok(devices) => {
                print_paired(&devices);
                println!("\nDone.");
                0
            }
            Err(e) => {
                eprintln!("BluetoothFindFirstDevice failed: {e}");
                1
            }
        };
    }
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        let _ = ctrlc::set_handler(move || running.store(false, Ordering::Release));
    }

    match (&a.hci_tcp, &a.usb) {
        (Some(addr), usb) => {
            println!("Transport: H4 over TCP to {addr} (development / testing)");
            if let Some((u, _)) = usb {
                println!("-u {u} is not used with --hci-tcp");
            }
        }
        (None, Some((u, _))) => println!("Transport: USB (WinUSB), adapter {u}"),
        (None, None) => println!("Transport: USB (WinUSB)"),
    }
    let capture = HciCapture::new();
    if let Some(path) = &a.hci_capture
        && let Err(e) = capture.start(path)
    {
        eprintln!("Cannot create HCI capture file {}: {e}", path.display());
        return 1;
    }
    let (tx, rx) = mpsc::channel();
    let service = Service::new(
        ServiceOptions {
            config_dir: a2dpwb_core::paths::config_dir(),
            hci_tcp: a.hci_tcp.clone(),
            capture: capture.clone(),
            afh: a.afh.clone(),
            // WIRING(-u): once ServiceOptions has `usb: hci_transport::UsbSelector`
            // (crates/core/src/service.rs), pass the adapter chosen with -u:
            // usb: a.usb.as_ref().map(|(_, s)| s.clone()).unwrap_or_default(),
        },
        tx,
    );

    if let Some(secs) = a.scan {
        let code = match service.scan(secs) {
            Ok(found) => {
                for d in found {
                    println!(
                        "{}  {:<32} CoD 0x{:06X}  RSSI {}{}",
                        bt_host::addr_str(&d.addr),
                        d.name,
                        d.class_of_device,
                        d.rssi,
                        if d.is_audio() { "  (audio)" } else { "" }
                    );
                }
                0
            }
            Err(m) => {
                eprintln!("{m}");
                1
            }
        };
        service.shutdown();
        capture.stop();
        return code;
    }

    // The measuring receiver: its IP for the statistics and, without -d, its
    // Bluetooth address as the device to stream to
    let receiver = a2dpwb_core::receiver::ReceiverClient::new();
    let mut device_arg = a.device.clone();
    if let Some(target) = &a.remote_sink {
        use a2dpwb_core::receiver;
        let Some((host, port)) = receiver::split_target(target) else {
            eprintln!("Bad --remote-sink {target}");
            return 2;
        };
        let auto = host.eq_ignore_ascii_case(receiver::AUTO);
        let found = receiver::discover((!auto).then_some(host.as_str()), port, receiver::DISCOVER_TIMEOUT);
        let mut stats_target = target.clone();
        match found.first() {
            None => println!("Receiver: none answered {}", if auto { "on the local network" } else { host.as_str() }),
            Some(r) => {
                println!("{}", r.describe(found.len() > 1));
                if device_arg.is_none() && !r.bt_address.is_empty() {
                    device_arg = Some(r.bt_address.clone());
                    println!("Streaming to the receiver (no -d given)");
                }
                if auto {
                    stats_target = format!("{}:{}", r.ip, r.port);
                }
            }
        }
        println!("Receiver statistics: {stats_target}");
        receiver.start(&stats_target);
    }

    let stop = |code: i32| {
        service.shutdown();
        capture.stop();
        if a.remote_sink.is_some() {
            receiver.stop();
        }
        code
    };
    let mut device_name = String::new();
    let device = match device_arg.as_deref() {
        Some(d) => match parse_device_address(d) {
            Some(addr) => addr,
            None => {
                eprintln!("Invalid Bluetooth address: {d}");
                return stop(1);
            }
        },
        // v1: the first Windows-paired device that is an A2DP sink by its
        // class of device (works while the adapter has WinUSB: Windows
        // remembers its paired devices)
        None => {
            println!("Scanning for Bluetooth audio devices...");
            let devices = match bt_paired::paired_devices() {
                Ok(devices) => devices,
                Err(e) => {
                    eprintln!("BluetoothFindFirstDevice failed: {e}");
                    Vec::new()
                }
            };
            print_paired(&devices);
            match bt_paired::first_a2dp_sink(&devices) {
                Some(d) => {
                    println!("Auto-selected device: {} ({})", d.name, d.address_string());
                    device_name = d.name.clone();
                    d.address
                }
                None => {
                    eprintln!(
                        "No Bluetooth audio device found.\n\
                         Use -d XX:XX:XX:XX:XX:XX to specify the device address.\n\
                         (Find the address in Windows Settings > Bluetooth before switching to WinUSB,\n\
                         or list the devices nearby with --scan)"
                    );
                    return stop(1);
                }
            }
        }
    };
    let source = if a.test_tone {
        SourceRequest::TestTone
    } else {
        match a.mode.as_str() {
            "virtual" => SourceRequest::VirtualDevice {
                id: a.audio_device.clone(),
                name: a.audio_device.clone(),
                auto_switch: true,
            },
            "app" => SourceRequest::Application { exe: a.app.clone() },
            _ => SourceRequest::SystemLoopback,
        }
    };
    println!("Target device: {}", bt_host::addr_str(&device));
    service.start_streaming(StreamRequest {
        device,
        device_name,
        codec: a.codec,
        codec_fallback: false,
        quality: a.quality,
        abr: a.abr,
        sample_rate: a.rate,
        bit_depth: a.bits,
        source,
        volume_sync: VolumeSync::Off,
        max_media_payload: a.max_packet,
    });

    let mut streaming_since: Option<Instant> = None;
    let mut last_summary = Instant::now();
    let mut code = 0;
    loop {
        if a.remote_sink.is_some() && streaming_since.is_some() && last_summary.elapsed() >= Duration::from_secs(5) {
            last_summary = Instant::now();
            println!("{}", receiver.summary());
        }
        if !running.load(Ordering::Acquire) {
            println!("\nStopping...");
            break;
        }
        if let (Some(since), Some(d)) = (streaming_since, a.duration)
            && since.elapsed() >= Duration::from_secs(d)
        {
            println!("\n--duration {d} s reached");
            break;
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(ServiceEvent::State(state, msg)) => {
                println!("[{state:?}] {msg}");
                match state {
                    StreamState::Streaming if streaming_since.is_none() => streaming_since = Some(Instant::now()),
                    StreamState::Error => {
                        code = 1;
                        break;
                    }
                    StreamState::Idle if streaming_since.is_some() || !service.is_running() => break,
                    _ => {}
                }
            }
            Ok(ServiceEvent::StreamInfo(i)) => println!(
                "Codec: {} | Bitrate: {} kbps | Sample rate: {} Hz | Channels: {}",
                i.codec, i.bitrate_kbps, i.sample_rate, i.channels
            ),
            Ok(ServiceEvent::Volume(v)) => println!("Headphone volume: {v} ({}%)", v as u32 * 100 / 127),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    service.shutdown();
    capture.stop();
    if a.remote_sink.is_some() {
        // Let the last packets arrive and the receiver report them
        std::thread::sleep(Duration::from_millis(1500));
    }
    let stats = service.stats();
    println!(
        "Sent {} media packets ({} bytes), {} dropped before the radio",
        stats.packets_sent.load(Ordering::Relaxed),
        stats.bytes_sent.load(Ordering::Relaxed),
        stats.queue_drops.load(Ordering::Relaxed) + stats.send_errors.load(Ordering::Relaxed)
    );
    if a.remote_sink.is_some() {
        println!("{}", receiver.summary());
        receiver.stop();
    }
    code
}

/// v1's listing of the Windows-paired devices (BtDeviceDiscovery::
/// scan_paired_devices)
fn print_paired(devices: &[PairedDevice]) {
    if devices.is_empty() {
        println!("No paired Bluetooth devices found");
        return;
    }
    for d in devices {
        println!("  [{}] {} ({})", if d.is_a2dp_sink() { "A2DP" } else { "----" }, d.name, d.address_string());
    }
    println!(
        "Found {} devices ({} with A2DP Sink)",
        devices.len(),
        devices.iter().filter(|d| d.is_a2dp_sink()).count()
    );
}

/// A Bluetooth address as v1 took it (BtDeviceDiscovery::normalize_address):
/// 12 hex digits, optionally with ':', '-', '.' or ' ' between the pairs
fn parse_device_address(s: &str) -> Option<[u8; 6]> {
    let mut digits = Vec::with_capacity(12);
    let mut after_sep = false;
    for c in s.trim().chars() {
        if let Some(v) = c.to_digit(16) {
            if digits.len() == 12 {
                return None;
            }
            digits.push(v as u8);
            after_sep = false;
        } else if matches!(c, ':' | '-' | '.' | ' ') {
            // Only between pairs, and not two in a row
            if digits.is_empty() || digits.len() % 2 != 0 || after_sep {
                return None;
            }
            after_sep = true;
        } else {
            return None;
        }
    }
    if digits.len() != 12 || after_sep {
        return None;
    }
    let mut a = [0u8; 6];
    for (i, b) in a.iter_mut().enumerate() {
        *b = digits[2 * i] << 4 | digits[2 * i + 1];
    }
    Some(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn list_and_adapter_options() {
        let a = parse(&args("--cli -l")).unwrap();
        assert!(a.list && a.usb.is_none() && a.device.is_none());

        let a = parse(&args(r"--cli -u USB\VID_0BDA&PID_8771\00E04C000001 -d 00:11:22:33:44:55")).unwrap();
        let (given, sel) = a.usb.unwrap();
        assert_eq!(given, r"USB\VID_0BDA&PID_8771\00E04C000001");
        assert_eq!(sel.instance_id.as_deref(), Some(r"USB\VID_0BDA&PID_8771\00E04C000001"));

        let a = parse(&args(r"--cli -u \\?\usb#vid_0bda&pid_8771#00e04c000001#{a5dcbf10-6530-11d2-901f-00c04fb951ed}"))
            .unwrap();
        assert_eq!(a.usb.unwrap().1.instance_id.as_deref(), Some(r"usb\vid_0bda&pid_8771\00e04c000001"));

        let a = parse(&args("--cli -u 0bda:8771")).unwrap();
        assert_eq!(a.usb.unwrap().1.vid_pid, Some((0x0BDA, 0x8771)));

        assert!(parse(&args("--cli -u nonsense")).err().unwrap_or_default().contains("not a USB device"));
        assert_eq!(parse(&args("--cli -u")).err().as_deref(), Some("-u needs a value"));
    }

    #[test]
    fn device_address_as_v1() {
        let want = Some([0x00, 0x1A, 0x7D, 0xDA, 0x71, 0x13]);
        for ok in ["00:1A:7D:DA:71:13", "00-1a-7d-da-71-13", "001A7DDA7113", "00.1A.7D.DA.71.13", " 00 1A 7D DA 71 13 "]
        {
            assert_eq!(parse_device_address(ok), want, "{ok}");
        }
        for bad in [
            "",
            "00:1A:7D:DA:71",
            "00:1A:7D:DA:71:13:00",
            "0:01A:7D:DA:71:13",
            "00::1A7DDA7113",
            "00:1A:7D:DA:71:13:",
            "00:1A:7D:DA:71:1G",
        ] {
            assert_eq!(parse_device_address(bad), None, "{bad}");
        }
    }
}
