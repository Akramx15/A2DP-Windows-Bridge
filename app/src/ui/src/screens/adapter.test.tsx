import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../app/App";
import { MockCore, type MockScenario } from "../ipc/mock";
import { createCore } from "../ipc/store";

let mock: MockCore | null = null;

function start(scenario: MockScenario = "normal") {
  location.hash = "#/adapter";
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ speed: 0, scenario, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  render(() => <App core={core} />);
  return { calls, mock };
}

afterEach(() => {
  cleanup();
  mock?.dispose();
  mock = null;
  vi.restoreAllMocks();
});

describe("Adapter firmware", () => {
  it("shows the chip's firmware and saves another chip", async () => {
    const { calls } = start();
    await screen.findByRole("heading", { level: 1, name: "Adapters" });
    await waitFor(() => expect(calls.some((c) => c.method === "adapter.firmware_files")).toBe(true));
    const chip = screen.getByLabelText("Bluetooth Chip") as HTMLSelectElement;
    expect(chip.options[chip.selectedIndex].text).toBe("Auto (RTL8761BU)");
    // In the card of the adapter it applies to, not a section of its own
    expect(chip.closest(".adapter-card")).not.toBeNull();
    expect(within(chip.closest(".adapter-card") as HTMLElement).getByRole("group", { name: "Bluetooth Firmware Selection" })).toContainElement(chip);
    // The section lists the files the detected chip needs
    expect(await screen.findByText("rtl8761bu_config.bin")).toBeInTheDocument();

    fireEvent.change(chip, { target: { selectedIndex: 1 } });
    await waitFor(() => expect(calls.some((c) => c.method === "settings.set")).toBe(true));
    const saved = calls.find((c) => c.method === "settings.set")!.params as {
      settings: { bt_chip_pid: number; bt_chip_fw_stem: string };
    };
    expect(saved.settings).toMatchObject({ bt_chip_pid: 0xfffe, bt_chip_fw_stem: "" });
    expect(await screen.findByText(/Realtek initialization is skipped/)).toBeInTheDocument();
    // The core checks the firmware for the new chip: none needed now
    expect(await screen.findByText("No Realtek files needed")).toBeInTheDocument();
    // The folder is scanned again after the change
    await waitFor(() => expect(calls.filter((c) => c.method === "adapter.firmware_files").length).toBeGreaterThan(1));
  });

  it("opens the firmware folder", async () => {
    const { mock } = start();
    const open = vi.spyOn(mock, "openFolder");
    fireEvent.click(await screen.findByRole("button", { name: "Open Firmware Folder" }));
    await waitFor(() => expect(open).toHaveBeenCalledWith("C:\\Users\\you\\AppData\\Roaming\\A2DPWB\\firmware"));
  });

  it("downloads the missing firmware after the user has read the terms", async () => {
    const { calls } = start();
    // A chip whose files are not in the sample folder
    const chip = (await screen.findByLabelText("Bluetooth Chip")) as HTMLSelectElement;
    fireEvent.change(chip, { target: { value: [...chip.options].find((o) => o.text.startsWith("RTL8852BU"))!.value } });
    expect(await screen.findByText("Firmware not found")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Download Firmware" }));
    const dialog = await screen.findByRole("dialog", { name: "Download the Realtek firmware" });
    expect(dialog).toHaveTextContent("LICENCE.rtlwifi_firmware.txt");
    expect(calls.some((c) => c.method === "adapter.download_firmware")).toBe(false);
    fireEvent.click([...dialog.querySelectorAll("button")].find((b) => b.textContent === "Download Firmware")!);
    await waitFor(() =>
      expect(calls.find((c) => c.method === "adapter.download_firmware")?.params).toEqual({ file: "rtl8852bu_fw.bin" }),
    );
    expect(await screen.findByText("Firmware downloaded. Bluetooth loads it at the next connection.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText("Firmware not found")).not.toBeInTheDocument());
  });

  /** Opens the download dialog for a chip with no files and starts the
   *  download, which the test answers itself */
  async function startDownload() {
    const { mock } = start();
    let answer!: { resolve: (v: unknown) => void; reject: (e: unknown) => void };
    vi.spyOn(mock as any, "downloadFirmware").mockImplementation(
      () => new Promise((resolve, reject) => (answer = { resolve, reject })),
    );
    const chip = (await screen.findByLabelText("Bluetooth Chip")) as HTMLSelectElement;
    fireEvent.change(chip, { target: { value: [...chip.options].find((o) => o.text.startsWith("RTL8852BU"))!.value } });
    expect(await screen.findByText("Firmware not found")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Download Firmware" }));
    const dialog = await screen.findByRole("dialog", { name: "Download the Realtek firmware" });
    fireEvent.click([...dialog.querySelectorAll("button")].find((b) => b.textContent === "Download Firmware")!);
    await waitFor(() => expect(answer).toBeDefined());
    return { dialog, answer: () => answer };
  }

  it("shows a spinner and stays open while downloading", async () => {
    const { dialog, answer } = await startDownload();
    expect(await within(dialog).findByRole("status")).toHaveTextContent("Downloading the firmware from kernel.org");
    expect(dialog.querySelector(".spinner.big")).not.toBeNull();
    // Neither Cancel, Escape nor the backdrop closes it meanwhile
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    expect(cancel).toBeDisabled();
    fireEvent.keyDown(dialog, { key: "Escape" });
    fireEvent.click(dialog.parentElement!);
    expect(screen.getByRole("dialog", { name: "Download the Realtek firmware" })).toBeInTheDocument();
    answer().resolve([]);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("does not call the UI's own timeout a failed download", async () => {
    const { answer } = await startDownload();
    answer().reject({ code: "ipc.timeout", message: { key: "ipc.error.timeout", args: ["adapter.download_firmware"] } });
    expect(await screen.findByText(/The download is taking a while/)).toBeInTheDocument();
    expect(screen.queryByText(/not responding/i)).not.toBeInTheDocument();
  });
});
