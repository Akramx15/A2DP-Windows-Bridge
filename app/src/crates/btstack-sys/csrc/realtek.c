/*
 * realtek.c - BTstack's Realtek chipset driver, plus a read of its USB
 * firmware table
 *
 * fw_patch_table_usb is static in btstack_chipset_realtek.c and BTstack only
 * exports its VID:PID pairs. The app also needs each PID's firmware name to
 * tell the chip family (0x4853 and 0xB85B are both RTL8852BU), so this file
 * compiles the driver itself and reads the table from the same unit.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "btstack_chipset_realtek.c"

/* The firmware name (no extension, e.g. "rtl8852bu_fw") of entry `index` of
 * btstack_chipset_realtek_get_vendor_product_id's table */
const char *a2dpwb_realtek_usb_fw_name(uint16_t index) {
    if (index >= btstack_chipset_realtek_get_num_usb_controllers()) return NULL;
    return fw_patch_table_usb[index].patch_name;
}
