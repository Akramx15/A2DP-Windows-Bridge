/*
 * Config path utility
 *
 * Returns the application data directory for persistent settings.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef CONFIG_PATH_H
#define CONFIG_PATH_H

#include <string>

/* Get (and create if needed) %APPDATA%\A2DPWB\ */
std::string get_config_dir();

#endif /* CONFIG_PATH_H */
