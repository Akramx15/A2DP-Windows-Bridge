"""CSIP cases and checks of run_test.py --le-audio.

The earbuds layout with --csip-set: the two earbuds (Front Left / Front
Right, one sink ASE each) are the members of one coordinated set. A2DPWB is
given only the first earbud's address (first_address_only); its CSIP module
reads the SIRK, finds the second earbud by its RSI, connects it and streams
left to one and right to the other.
"""

import re

from .case import MONO_TONES, Case

TWO_CIS = r'LC3 48000 Hz 10 ms 155 octets, 2 channel\(s\) over 2 CIS'
ONE_CIS = r'LC3 48000 Hz 10 ms 155 octets, 1 channel\(s\) over 1 CIS'

CASES = [
    # Plaintext SIRK: A2DPWB finds and connects the second earbud itself
    Case('csip-set', 'earbuds', TWO_CIS, sink_args=['--csip-set', 'plaintext'],
         tags=('csip',), first_address_only=True),
    # Encrypted SIRK (decrypted with the LTK); the earbud named first has
    # rank 2, so the found one (rank 1) must come first
    Case('csip-encrypted-sirk', 'earbuds', TWO_CIS, sink_args=['--csip-set', 'encrypted', '--csip-ranks', '2,1'],
         tags=('csip',), first_address_only=True),
    # The second earbud never advertises: stream to the first one alone
    # (mono downmix), with a warning
    Case('csip-missing-member', 'earbuds', ONE_CIS, tones=MONO_TONES,
         sink_args=['--csip-set', 'plaintext', '--csip-missing'], tags=('csip',), first_address_only=True),
    # Both earbuds named (-d / --le-device) and a set: no member twice
    Case('csip-both-named', 'earbuds', TWO_CIS, sink_args=['--csip-set', 'plaintext'], tags=('csip',)),
    # The named earbud drops its link while streaming: A2DPWB reconnects it,
    # keeps the other (still connected) and streams to both again
    Case('csip-link-loss', 'earbuds', TWO_CIS, sink_args=['--csip-set', 'plaintext', '--drop-link-after', '5'],
         tags=('csip',), first_address_only=True, streams=2, lossy=True, duration=14),
]


def check(case, ctx):
    if 'csip' not in case.tags:
        return []
    log = ctx['a2dpwb_log']
    sink_log = ctx['sink_log']
    failures = []
    if 'CSIP: crypto self-check passed' not in log:
        failures.append('CSIP crypto self-check did not pass')
    first, second = ctx['sink_addresses'][0], ctx['sink_addresses'][1]
    sirk_type = 'encrypted' if 'encrypted' in case.sink_args else 'plaintext'
    for addr in (first, second) if case.name != 'csip-missing-member' else (first,):
        if not re.search(re.escape(addr) + r': member of coordinated set 0, rank \d of 2 \(' + sirk_type, log):
            failures.append(f'CSIP: {addr} not read as a member of the set ({sirk_type} SIRK)')
    if f'CSIS SIRK read ({sirk_type})' not in sink_log:
        failures.append(f'the sinks did not serve a {sirk_type} SIRK')

    found = re.search(re.escape(second) + r' "[^"]*" resolves to coordinated set 0 \(RSI\)', log)
    if case.first_address_only and case.name != 'csip-missing-member' and not found:
        failures.append(f'CSIP did not find {second} by its RSI')
    if case.name == 'csip-both-named' and found:
        failures.append('CSIP scanned for a member that was named already')

    if case.name == 'csip-missing-member':
        if 'CSIP: only 1 of 2 members of coordinated set 0 found' not in log:
            failures.append('no warning about the missing set member')
        if 'streaming to coordinated set 0, 1 of 2 members' not in log:
            failures.append('CSIP did not stream to the member available')
        if found:
            failures.append('the missing member was found after all')
        return failures

    # The set: both members, ordered by rank
    m = re.findall(r'CSIP: streaming to coordinated set 0, 2 of 2 members: (\S+) rank (\d), (\S+) rank (\d)', log)
    if len(m) != case.streams:
        failures.append(f'CSIP: {len(m)} set-ups with both members (expected {case.streams})')
    for a1, r1, a2, r2 in m:
        if (r1, r2) != ('1', '2') or {a1, a2} != {first, second}:
            failures.append(f'CSIP: members not ordered by rank: {a1} rank {r1}, {a2} rank {r2}')
    if case.name == 'csip-link-loss':
        if 'LE Audio stream restored' not in log:
            failures.append('the stream was not set up again after the link loss')
        if not re.search(re.escape(first) + r' \(rank \d of coordinated set 0\) disconnected', log):
            failures.append('CSIP did not see the member disconnect')
    return failures
