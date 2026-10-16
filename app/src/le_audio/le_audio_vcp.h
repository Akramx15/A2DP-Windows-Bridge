/*
 * LE Audio (experimental) - VCP Volume Controller (stub)
 *
 * Owner: the VCP work package. See le_audio_vcp.cpp for the plan.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_VCP_H
#define LE_AUDIO_VCP_H

#include "le_audio_module.h"

namespace le_audio {

/* Registered in le_audio_modules.cpp */
Module *vcp_module();

} /* namespace le_audio */

#endif /* LE_AUDIO_VCP_H */
