// SPDX-License-Identifier: MPL-2.0
// Console setup: Wave's println writes UTF-8 bytes through printf. Windows
// consoles default to a legacy code page (CP949 on Korean systems), which
// garbles them, so switch the console to UTF-8. Elsewhere this is a no-op.
//
// The two kernel32 functions are declared here instead of including
// <windows.h>, so no Windows SDK is needed to compile this file.

#include "wjson_native.h"

#ifdef _WIN32
extern "C" __declspec(dllimport) unsigned int __stdcall GetConsoleOutputCP(void);
extern "C" __declspec(dllimport) int __stdcall SetConsoleOutputCP(unsigned int code_page);
#endif

extern "C" int32_t wjn_console_utf8(void) WJN_NOEXCEPT {
#ifdef _WIN32
    const unsigned int previous = GetConsoleOutputCP();
    SetConsoleOutputCP(65001);  // CP_UTF8
    return static_cast<int32_t>(previous);
#else
    return 0;
#endif
}
