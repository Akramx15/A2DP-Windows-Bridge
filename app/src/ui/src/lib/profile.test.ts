// The plain words a profile shows: the summary line of the card, the kind
// of use ("headphones", "AutoEQ", ...) and the engine it runs.

import { describe, expect, it } from "vitest";
import { t } from "../i18n";
import { sampleProfile } from "../ipc/mock";
import type { SpatialEngine } from "../ipc/types";
import { engineLabelOf, profileSummary, purposeLabel } from "./profile";

describe("the summary of a profile", () => {
  it("names the device, the codec with its quality and the capture source", () => {
    expect(profileSummary(sampleProfile(), t)).toBe("WH-1000XM5 · LDAC High (990 kbps) · System Loopback");
  });

  it("names the application it captures", () => {
    const p = sampleProfile({ capture_mode: "app", app_exe: "Spotify.exe", app_name: "Spotify" });
    expect(profileSummary(p, t)).toBe("WH-1000XM5 · LDAC High (990 kbps) · Spotify");
  });

  it("falls back to the address when the device has no name", () => {
    expect(profileSummary(sampleProfile({ device_name: "" }), t)).toContain("AC:80:0A:12:34:56");
  });

  it("names LC3 for an LE Audio profile (no quality of its own)", () => {
    expect(profileSummary(sampleProfile({ le_audio: true }), t)).toBe("WH-1000XM5 · LC3 (LE Audio) · System Loopback");
  });
});

describe("the purpose of a profile", () => {
  it("names the kinds of use", () => {
    expect(purposeLabel("headphones", t)).toBe("Headphones");
    expect(purposeLabel("autoeq", t)).toBe("AutoEQ");
    expect(purposeLabel("speaker", t)).toBe("Speaker");
    expect(purposeLabel("custom", t)).toBe("Custom");
  });

  it("has no pill when it is not set, and keeps an unknown kind as it is", () => {
    expect(purposeLabel("", t)).toBe("");
    expect(purposeLabel("tv", t)).toBe("tv");
  });
});

describe("the engine of a profile", () => {
  const engines: SpatialEngine[] = [
    { id: "sony", label: "Sony native (reference)", path: "C:\\engines\\sony.exe", family: "sony" },
    { id: "hpp", label: "Personalized HPP + XM6 BA", path: "C:\\engines\\hpp.exe", family: "sony" },
  ];

  it("names the stage the profile runs", () => {
    const p = sampleProfile({ spatial_engine: true, spatial_engine_path: "C:\\engines\\hpp.exe" });
    expect(engineLabelOf(p, engines)).toBe("Personalized HPP + XM6 BA");
  });

  it("has no name when the stage is not among the ones the core lists", () => {
    const p = sampleProfile({ spatial_engine: true, spatial_engine_path: "C:\\engines\\other.exe" });
    expect(engineLabelOf(p, engines)).toBe("");
    expect(engineLabelOf(sampleProfile(), engines)).toBe("");
  });
});
