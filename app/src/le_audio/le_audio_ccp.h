/*
 * LE Audio (experimental) - CCP: Generic Telephone Bearer Service on the PC (stub)
 *
 * Owner: the CCP work package. See le_audio_ccp.cpp for the plan.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_CCP_H
#define LE_AUDIO_CCP_H

#include "le_audio_module.h"

namespace le_audio {

/* Registered in le_audio_modules.cpp */
Module *ccp_module();

} /* namespace le_audio */

#endif /* LE_AUDIO_CCP_H */
