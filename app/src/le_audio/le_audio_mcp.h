/*
 * LE Audio (experimental) - MCP: Generic Media Control Service on the PC
 *
 * The PC is the Media Control Server (GMCS, CCID_GMCS): see le_audio_mcp.cpp
 * for the service and le_audio_mcp_media.h for the Windows side.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_MCP_H
#define LE_AUDIO_MCP_H

#include "le_audio_module.h"

namespace le_audio {

/* Registered in le_audio_modules.cpp */
Module *mcp_module();

} /* namespace le_audio */

#endif /* LE_AUDIO_MCP_H */
