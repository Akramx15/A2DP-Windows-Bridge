// The SBC bit rate as the core works it out (crates/codec/src/sbc/mod.rs
// tests have the same numbers)

import { describe, expect, it } from "vitest";
import { sampleDevice, sampleProfile, XM5_SBC } from "../ipc/mock";
import { knownSbcCaps, sbcFrameLength, sbcKbps, sbcOffered, sbcRates, sbcSetup } from "./sbc";

const sbc = (over: Partial<ReturnType<typeof sampleProfile>> = {}) => sampleProfile({ codec: "sbc", ...over });

describe("SBC frame length and bit rate", () => {
  it("follows the A2DP specification's formula", () => {
    // (rate, blocks, subbands, mode, bitpool, frame length, kbps)
    const cases: [number, number, number, string, number, number, number][] = [
      [44100, 16, 8, "joint_stereo", 53, 119, 327],
      [44100, 16, 8, "stereo", 53, 118, 325],
      [44100, 16, 8, "dual_channel", 32, 140, 385],
      [44100, 16, 8, "mono", 31, 70, 192],
      [44100, 8, 4, "joint_stereo", 35, 44, 485],
      [48000, 8, 4, "stereo", 35, 43, 516],
      [44100, 4, 8, "joint_stereo", 53, 40, 441],
      [48000, 12, 8, "joint_stereo", 53, 93, 372],
      [48000, 12, 4, "dual_channel", 20, 68, 544],
      [44100, 4, 4, "mono", 30, 21, 463],
    ];
    for (const [rate, blocks, subbands, mode, bitpool, len, kbps] of cases) {
      expect(sbcFrameLength(mode, blocks, subbands, bitpool)).toBe(len);
      expect(sbcKbps(len, rate, blocks, subbands)).toBe(kbps);
    }
  });

  it("works out a profile's setup as the core", () => {
    // Auto: joint stereo, 16 blocks, 8 subbands, the quality's bitpool
    expect(sbcSetup(sbc(), 44100, null)).toMatchObject({ blocks: 16, subbands: 8, mode: "joint_stereo", bitpool: 53, kbps: 327 });
    expect(sbcSetup(sbc({ quality: "sq" }), 48000, null)).toMatchObject({ bitpool: 35, kbps: 249 });
    // Dual channel halves the auto bitpool per channel; a bitpool asked for wins
    expect(sbcSetup(sbc({ sbc_channel_mode: "dual_channel" }), 44100, null)).toMatchObject({ bitpool: 27, kbps: 330 });
    expect(sbcSetup(sbc({ sbc_bitpool: 40, sbc_blocks: 8, sbc_subbands: 4 }), 44100, null)).toMatchObject({ frameBytes: 49, kbps: 540 });
    // SBC XQ: dual channel whatever was asked, its own bitpools
    const xq = sampleProfile({ codec: "sbcxq", sbc_channel_mode: "stereo" });
    expect(sbcSetup(xq, 44100, null)).toMatchObject({ mode: "dual_channel", bitpool: 47, frameBytes: 200, kbps: 551 });
    expect(sbcSetup({ ...xq, sbcxq_bitpool: 53 }, 48000, null)).toMatchObject({ bitpool: 43 });
  });

  it("takes what the device does not list as the default, and its bitpool range", () => {
    const caps = { ...XM5_SBC, max_bitpool: 35 };
    const s = sbcSetup(sbc({ sbc_subbands: 4, sbc_allocation: "snr", sbc_blocks: 8 }), 44100, caps);
    expect(s).toMatchObject({ blocks: 8, subbands: 8, allocation: "loudness", bitpool: 35 });
    expect(sbcOffered(caps, "subbands", 4)).toBe(false);
    expect(sbcOffered(caps, "blocks", 4)).toBe(true);
    expect(sbcOffered(null, "subbands", 4)).toBe(true);
    expect(sbcRates({ sample_rate: 0 }, { ...caps, sample_rates: [48000] })).toEqual([48000]);
    expect(sbcRates({ sample_rate: 44100 }, null)).toEqual([44100]);
    expect(sbcRates({ sample_rate: 0 }, null)).toEqual([44100, 48000]);
  });

  it("finds the caps of the profile's device", () => {
    const list = [sampleDevice({ address: "AC:80:0A:12:34:56", sbc: XM5_SBC }), sampleDevice({ address: "00:1A:7D:DA:71:13" })];
    expect(knownSbcCaps(list, "ac-80-0a-12-34-56")).toEqual(XM5_SBC);
    expect(knownSbcCaps(list, "00:1A:7D:DA:71:13")).toBeNull();
    expect(knownSbcCaps(list, "nope")).toBeNull();
  });
});
