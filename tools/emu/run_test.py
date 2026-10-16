"""End-to-end A2DP test of A2DPWB against a virtual sink - no Bluetooth hardware.

    python tools/emu/setup_env.py          (once)
    python tools/emu/run_test.py [--codecs sbc aac ...] [--duration 5]

For each codec: starts emu_sink.py (Bumble virtual link + A2DP sink), runs
A2DPWB.exe --cli with a test tone over H4/TCP, then checks both HCI captures
with a2dpwb_decode:
  - A2DPWB streams the requested codec and a2dpwb_decode finds no problem,
    on the sent side and on the received side
  - the sink received every media packet / frame A2DPWB sent
  - decoded audio (all codecs but LDAC and LHDC V5, which have no open-source
    decoder) is identical on both sides and is the test tone: 1 kHz left,
    1.5 kHz right, amplitude 0.5

Outputs (captures, logs, reports, WAV) go to tools/emu/out/<codec>/.
Exit code 0 when every codec passed. Uses only the Python standard library;
the sink runs in the tools/emu/.venv environment.

    python tools/emu/run_test.py --le-audio [case ...]

runs the experimental LE Audio cases instead (le_audio_tests/: stereo,
two-ase, earbuds, mono-16k and the profiles' own): emu_le_audio_sink.py with
the case's layout, A2DPWB.exe --cli --le-audio with the test tone, and
a2dpwb_decode --le-audio on both sides:
  - one LC3 stream of the expected configuration on both sides, consecutive
    ISO SDU sequence numbers, no decoder problems
  - the sinks received every SDU A2DPWB sent; decoded audio identical on both
    sides and the test tone (1 kHz left, 1.5 kHz right; mono: both)
  - the checks of le_audio_tests/<profile>.py
Outputs go to tools/emu/out/le-<case>/.
"""

import argparse
import math
import os
import queue
import re
import shutil
import struct
import subprocess
import sys
import threading
import time
import wave

import le_audio_tests

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
VENV_PYTHON = os.path.join(HERE, '.venv', 'Scripts', 'python.exe')
if not os.path.exists(VENV_PYTHON) and sys.prefix != sys.base_prefix:
    # No .venv here (e.g. a git worktree): run the sink with the virtual
    # environment this script runs in (one with bumble installed)
    VENV_PYTHON = sys.executable
SINK_ADDRESS = '00:A2:D0:00:00:02'
BASE_PORT = 9101
LE_BASE_PORT = 9201

CODECS = ['sbc', 'aac', 'aptx', 'aptxhd', 'aptxll', 'ldac', 'lhdcv5']
CONFIG_PATTERN = {  # a2dpwb_decode's "Configuration :" line
    'sbc': r'SBC \d', 'aac': r'AAC ', 'aptx': r'aptX \d', 'aptxhd': r'aptX HD \d',
    'aptxll': r'aptX LL \d', 'ldac': r'LDAC \d', 'lhdcv5': r'LHDC V5 \d',
}
NO_DECODER = {'ldac', 'lhdcv5'}  # frame headers are validated, audio is not decoded
TONES = ((1000.0, 0.5), (1500.0, 0.5))  # (frequency, amplitude) per channel, as --test-tone


def parse_report(text):
    """First stream of an a2dpwb_decode report."""
    def find(pattern, cast=str):
        m = re.search(pattern, text, re.M)
        return cast(m.group(1)) if m else None
    return {
        'config': find(r'Configuration : (.*)$'),
        'packets': find(r'Media packets : (\d+)', int),
        'frames': find(r'Frames\s+: (\d+)', int),
        'audio_s': find(r'Audio\s+: ([\d.]+) s', float),
        'wav': find(r'WAV\s+: (.*)$'),
        'streams': len(re.findall(r'^=== Stream ', text, re.M)),
        'result_ok': re.search(r'^RESULT: OK', text, re.M) is not None,
        'problems': re.findall(r'^    - (.*)$', text, re.M),
    }


def read_wav(path):
    with wave.open(path) as w:
        ch, width, rate, n = w.getnchannels(), w.getsampwidth(), w.getframerate(), w.getnframes()
        raw = w.readframes(n)
    full = float(1 << (8 * width - 1))
    samples = [[] for _ in range(ch)]
    for i in range(n * ch):
        b = raw[i * width:(i + 1) * width]
        if width == 3:
            b += b'\xff' if b[2] & 0x80 else b'\x00'
        samples[i % ch].append(struct.unpack('<h' if width == 2 else '<i', b)[0] / full)
    return rate, samples


def tone_check(path, tones=None):
    """Returns (ok, description) for the test tone in the middle of the WAV.
    tones: per channel the (frequency, amplitude) pairs expected there
    (default: TONES, one per channel)."""
    tones = tones or tuple((t,) for t in TONES)
    rate, channels = read_wav(path)
    if len(channels) != 2 or len(channels[0]) < rate:
        return False, f'expected >= 1 s of stereo audio, got {len(channels)} ch'
    start, count = len(channels[0]) // 2 - rate // 4, rate // 2
    notes, ok = [], True
    for xs, expected in zip(channels, tones):
        xs = xs[start:start + count]
        mean_sq = sum(x * x for x in xs) / count
        energy = 0.0
        for freq, amp in expected:
            k = 2 * math.cos(2 * math.pi * freq / rate)
            s1 = s2 = 0.0
            for x in xs:
                s1, s2 = x + k * s1 - s2, s1
            tone_amp = 2 * math.sqrt(max(s1 * s1 + s2 * s2 - k * s1 * s2, 0.0)) / count
            energy += tone_amp * tone_amp / 2
            ok &= abs(tone_amp - amp) < 0.05
            notes.append(f'{freq:.0f} Hz amp {tone_amp:.3f}')
        share = energy / mean_sq if mean_sq else 0.0
        ok &= share > 0.95
        notes[-1] += f' ({100 * share:.1f}%)'
    return ok, ', '.join(notes)


def start_sink(port, capture, log_path, script='emu_sink.py', extra=()):
    """Starts a virtual sink; its READY line is in proc.ready_line."""
    log = open(log_path, 'w', encoding='utf-8')
    proc = subprocess.Popen(
        [VENV_PYTHON, '-u', os.path.join(HERE, script), '--port', str(port), '--capture', capture]
        + list(extra),
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding='utf-8', errors='replace')
    lines = queue.Queue()

    def pump():
        for line in proc.stdout:
            log.write(line)
            log.flush()
            lines.put(line)
        log.close()
    threading.Thread(target=pump, daemon=True).start()
    deadline = time.time() + 30
    while time.time() < deadline:
        try:
            line = lines.get(timeout=0.5)
            if line.startswith('READY'):
                proc.ready_line = line.strip()
                return proc
        except queue.Empty:
            if proc.poll() is not None:
                break
    proc.kill()
    raise RuntimeError(f'virtual sink did not start, see {log_path}')


def run_codec(args, codec, port):
    out = os.path.join(args.out, codec)
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    src_capture = os.path.join(out, 'a2dpwb.pklg')
    sink_capture = os.path.join(out, 'sink.pklg')
    failures = []

    sink = start_sink(port, sink_capture, os.path.join(out, 'sink.log'))
    try:
        env = dict(os.environ, A2DPWB_CONFIG_DIR=os.path.join(out, 'config'))  # isolated link keys
        with open(os.path.join(out, 'a2dpwb.log'), 'w', encoding='utf-8', errors='replace') as log:
            try:
                rc = subprocess.run(
                    [args.a2dpwb, '--cli', '--hci-tcp', f'127.0.0.1:{port}', '-d', SINK_ADDRESS,
                     '-c', codec, '--test-tone', '--duration', str(args.duration),
                     '--hci-capture', src_capture]
                    + (['--max-packet', str(args.max_packet)] if args.max_packet else []),
                    stdout=log, stderr=subprocess.STDOUT, env=env, timeout=args.duration + 90).returncode
            except subprocess.TimeoutExpired:
                rc = 'timeout'
        if rc != 0:
            failures.append(f'A2DPWB exited with {rc} (see a2dpwb.log)')
    finally:
        sink.terminate()
        sink.wait(10)

    reports = {}
    for side, capture, extra in (('sent', src_capture, []), ('received', sink_capture, ['--received'])):
        if not os.path.exists(capture):
            failures.append(f'{side}: no capture')
            continue
        text = subprocess.run([args.decoder, capture, '-o', os.path.join(out, side)] + extra,
                              capture_output=True, text=True, encoding='utf-8', errors='replace').stdout
        with open(os.path.join(out, f'{side}_report.txt'), 'w', encoding='utf-8') as f:
            f.write(text)
        r = reports[side] = parse_report(text)
        if r['streams'] != 1:
            failures.append(f'{side}: {r["streams"]} streams (expected 1)')
        if not re.match(CONFIG_PATTERN[codec], r['config'] or ''):
            failures.append(f'{side}: configuration "{r["config"]}"')
        if not r['result_ok']:
            failures.extend(f'{side}: {p}' for p in r['problems'] or ['a2dpwb_decode reported problems'])

    sent, received = reports.get('sent'), reports.get('received')
    detail = ''
    if sent and received:
        if (sent['packets'], sent['frames']) != (received['packets'], received['frames']):
            failures.append(f'sent {sent["packets"]} packets / {sent["frames"]} frames, '
                            f'received {received["packets"]} / {received["frames"]}')
        if (sent['audio_s'] or 0) < 0.9 * args.duration:
            failures.append(f'only {sent["audio_s"]} s of audio for --duration {args.duration}')
        detail = f'{sent["packets"]} packets, {sent["frames"]} frames, {sent["audio_s"]} s'
        if codec not in NO_DECODER:
            if not (sent['wav'] and received['wav']):
                failures.append('no decoded WAV')
            else:
                with open(sent['wav'], 'rb') as a, open(received['wav'], 'rb') as b:
                    if a.read() != b.read():
                        failures.append('decoded audio differs between sent and received side')
                ok, notes = tone_check(received['wav'])
                detail += f' | {notes}'
                if not ok:
                    failures.append(f'test tone not found: {notes}')
    return failures, detail


def parse_le_report(text):
    """a2dpwb_decode --le-audio report: as parse_report, plus the CIS lines"""
    r = parse_report(text)
    r['cis'] = re.findall(r'^  CIS 0x[0-9A-F]+ +: (.*)$', text, re.M)
    return r


def run_le_case(args, case, port):
    out = os.path.join(args.out, 'le-' + case.name)
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    src_capture = os.path.join(out, 'a2dpwb.pklg')
    sink_capture = os.path.join(out, 'sink.pklg')
    failures = []

    sink = start_sink(port, sink_capture, os.path.join(out, 'sink.log'), 'emu_le_audio_sink.py',
                      ['--layout', case.layout] + case.sink_args)
    addresses = sink.ready_line.split('sinks=')[1].split(',')
    try:
        env = dict(os.environ, A2DPWB_CONFIG_DIR=os.path.join(out, 'config'))  # isolated bonds
        cmd = [args.a2dpwb, '--cli', '--hci-tcp', f'127.0.0.1:{port}', '--le-audio', '-d', addresses[0]]
        for a in addresses[1:]:
            cmd += ['--le-device', a]
        cmd += ['--test-tone', '--duration', str(args.duration), '--hci-capture', src_capture] + case.a2dpwb_args
        with open(os.path.join(out, 'a2dpwb.log'), 'w', encoding='utf-8', errors='replace') as log:
            try:
                rc = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, env=env,
                                    timeout=args.duration + 120).returncode
            except subprocess.TimeoutExpired:
                rc = 'timeout'
        if rc != 0:
            failures.append(f'A2DPWB exited with {rc} (see a2dpwb.log)')
    finally:
        sink.terminate()
        sink.wait(10)

    sink_captures = [sink_capture] + [os.path.join(out, f'sink-{i + 1}.pklg') for i in range(1, len(addresses))]
    reports = {}
    for side, captures, extra in (('sent', [src_capture], []), ('received', sink_captures, ['--received'])):
        missing = [c for c in captures if not os.path.exists(c)]
        if missing:
            failures.append(f'{side}: no capture {missing}')
            continue
        text = subprocess.run([args.decoder, '--le-audio'] + captures + ['-o', os.path.join(out, side)] + extra,
                              capture_output=True, text=True, encoding='utf-8', errors='replace').stdout
        with open(os.path.join(out, f'{side}_report.txt'), 'w', encoding='utf-8') as f:
            f.write(text)
        r = reports[side] = parse_le_report(text)
        if r['streams'] != 1:
            failures.append(f'{side}: {r["streams"]} streams (expected 1)')
        if not re.match(case.config, r['config'] or ''):
            failures.append(f'{side}: configuration "{r["config"]}"')
        if not r['result_ok']:
            failures.extend(f'{side}: {p}' for p in r['problems'] or ['a2dpwb_decode reported problems'])

    sent, received = reports.get('sent'), reports.get('received')
    detail = ''
    if sent and received:
        if (sent['packets'], sent['frames']) != (received['packets'], received['frames']):
            failures.append(f'sent {sent["packets"]} SDUs / {sent["frames"]} frames, '
                            f'received {received["packets"]} / {received["frames"]}')
        if (sent['audio_s'] or 0) < 0.9 * args.duration:
            failures.append(f'only {sent["audio_s"]} s of audio for --duration {args.duration}')
        detail = f'{sent["config"]}: {sent["packets"]} SDUs, {sent["frames"]} LC3 frames, {sent["audio_s"]} s'
        if not (sent['wav'] and received['wav']):
            failures.append('no decoded WAV')
        else:
            with open(sent['wav'], 'rb') as a, open(received['wav'], 'rb') as b:
                if a.read() != b.read():
                    failures.append('decoded audio differs between sent and received side')
            if case.tones:
                ok, notes = tone_check(received['wav'], case.tones)
                detail += f' | {notes}'
                if not ok:
                    failures.append(f'test tone not found: {notes}')

    def read(path):
        try:
            with open(path, encoding='utf-8', errors='replace') as f:
                return f.read()
        except OSError:
            return ''
    a2dpwb_log = read(os.path.join(out, 'a2dpwb.log'))
    m = re.search(r'LE Audio: (\d+) SDU sets sent .*?(\d+) silence, (\d+) underruns', a2dpwb_log)
    if m:
        detail += f' | {m.group(2)} silence sets, {m.group(3)} underruns'
    ctx = dict(out=out, a2dpwb_log=a2dpwb_log, sink_log=read(os.path.join(out, 'sink.log')),
               sent=sent, received=received, sink_addresses=addresses)
    failures.extend(le_audio_tests.check(case, ctx))
    return failures, detail


def main_le(args):
    all_cases = le_audio_tests.cases()
    names = [c.name for c in all_cases]
    wanted = args.le_audio or names
    unknown = [n for n in wanted if n not in names]
    if unknown:
        sys.exit(f'Unknown LE Audio case(s) {unknown}; known: {names}')
    results = []
    for index, case in enumerate(c for c in all_cases if c.name in wanted):
        print(f'--- LE Audio {case.name} ...', flush=True)
        failures, detail = run_le_case(args, case, LE_BASE_PORT + index)
        results.append((case.name, failures))
        print(f'    {"PASS" if not failures else "FAIL"}  {detail}')
        for failure in failures:
            print(f'      - {failure}')
    passed = sum(1 for _, f in results if not f)
    print(f'\n{passed}/{len(results)} LE Audio cases passed. Details: {args.out}')
    sys.exit(0 if passed == len(results) else 1)


def main():
    parser = argparse.ArgumentParser(description='End-to-end A2DP test of A2DPWB against a virtual sink')
    parser.add_argument('--codecs', nargs='+', choices=CODECS, default=CODECS)
    parser.add_argument('--le-audio', nargs='*', metavar='CASE',
                        help='run the LE Audio (experimental) cases instead (default: all of them)')
    parser.add_argument('--duration', type=int, default=5, help='seconds of streaming per codec')
    parser.add_argument('--max-packet', type=int, help='passed to A2DPWB --max-packet (default: its own)')
    parser.add_argument('--build-dir', default=os.path.join(REPO, 'build'))
    parser.add_argument('--config', default='Release', help='build configuration (Release / Debug)')
    parser.add_argument('--out', default=os.path.join(HERE, 'out'))
    args = parser.parse_args()
    args.a2dpwb = os.path.join(args.build_dir, 'app', args.config, 'A2DPWB.exe')
    args.decoder = os.path.join(args.build_dir, 'tools', 'a2dp_decode', args.config, 'a2dpwb_decode.exe')
    for path, hint in ((VENV_PYTHON, 'run: python tools/emu/setup_env.py'),
                       (args.a2dpwb, 'build A2DPWB'), (args.decoder, 'build a2dpwb_decode')):
        if not os.path.exists(path):
            sys.exit(f'Missing {path} ({hint})')
    if args.le_audio is not None:
        main_le(args)

    results = []
    for index, codec in enumerate(args.codecs):
        print(f'--- {codec} ...', flush=True)
        failures, detail = run_codec(args, codec, BASE_PORT + index)
        results.append((codec, failures))
        print(f'    {"PASS" if not failures else "FAIL"}  {detail}')
        for failure in failures:
            print(f'      - {failure}')

    passed = sum(1 for _, f in results if not f)
    print(f'\n{passed}/{len(results)} codecs passed. Details: {args.out}')
    sys.exit(0 if passed == len(results) else 1)


if __name__ == '__main__':
    main()
