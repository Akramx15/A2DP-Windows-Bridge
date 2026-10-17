//! The command line: `a2dpwb-core --cli -d XX:XX:XX:XX:XX:XX [options]`.
//! The options are v1's; streaming goes through the same core service as the
//! resident process.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use a2dpwb_core::{Service, ServiceEvent, ServiceOptions, SourceRequest, StreamRequest, StreamState, VolumeSync};
use bt_host::HciCapture;
use bt_host::media::clamp_media_payload_limit;
use codec::{CodecKind, Quality};

const USAGE: &str = "\
Usage: a2dpwb-core --cli -d XX:XX:XX:XX:XX:XX [options]

  -d <address>          Bluetooth address of the headphones / receiver
  -c <codec>            ldac, aptxhd, aptxll, aptx, sbc, aac, lhdcv5, auto (default)
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
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        let _ = ctrlc::set_handler(move || running.store(false, Ordering::Release));
    }

    match &a.hci_tcp {
        Some(addr) => println!("Transport: H4 over TCP to {addr} (development / testing)"),
        None => println!("Transport: USB (WinUSB)"),
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

    let Some(device) = device_arg.as_deref().and_then(bt_host::parse_addr) else {
        eprintln!("No device: use -d XX:XX:XX:XX:XX:XX (find the address with --scan)");
        return 1;
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
        device_name: String::new(),
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
