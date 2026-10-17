// Bluetooth chips for the firmware (settings.bt_chip_pid / bt_chip_fw_stem),
// the table of v1 (app/src/bt_adapter_enum.cpp chip_db) and the core
// (crates/bt-host/src/chips.rs).

export const CHIP_AUTO = 0;
export const CHIP_NOT_REALTEK = 0xfffe;
export const CHIP_CUSTOM = 0xffff;

export interface Chip {
  pid: number;
  name: string;
  /** Firmware file stem: <stem>_fw.bin, <stem>_config.bin */
  stem: string;
}

export const REALTEK_CHIPS: Chip[] = [
  { pid: 0x8771, name: "RTL8761BU", stem: "rtl8761bu" },
  { pid: 0x876c, name: "RTL8761CU", stem: "rtl8761cu" },
  { pid: 0xb82c, name: "RTL8822BU", stem: "rtl8822bu" },
  { pid: 0xc821, name: "RTL8821CU", stem: "rtl8821cu" },
  { pid: 0xc82c, name: "RTL8822CU", stem: "rtl8822cu" },
  { pid: 0xd723, name: "RTL8723DU", stem: "rtl8723du" },
  { pid: 0xc852, name: "RTL8852AU", stem: "rtl8852au" },
  { pid: 0xb85b, name: "RTL8852BU", stem: "rtl8852bu" },
  { pid: 0xb85d, name: "RTL8852BTU", stem: "rtl8852btu" },
  { pid: 0xc85a, name: "RTL8852CU", stem: "rtl8852cu" },
  { pid: 0xb083, name: "RTL8851BU", stem: "rtl8851bu" },
  { pid: 0x922a, name: "RTL8922AU", stem: "rtl8922au" },
];

/** The settings.bt_chip_fw_stem that goes with a chip choice (as v1 stored it) */
export function stemFor(pid: number): string {
  if (pid === CHIP_CUSTOM) return "custom";
  return REALTEK_CHIPS.find((c) => c.pid === pid)?.stem ?? "";
}
