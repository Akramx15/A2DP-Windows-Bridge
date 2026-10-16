# a2dpwb_decode - offline checker / decoder for the media stream recorded in
# an A2DPWB debug-mode HCI capture (.pklg). Developer tool: not part of
# the release package.
#
# Links only codec libraries (Bluedroid SBC decoder, fdk-aac, libopenaptx,
# liblc3 for LE Audio), not BTstack. See THIRD_PARTY_LICENSES.md,
# "Developer tool: a2dpwb_decode".

if(NOT (TARGET sbc_decoder AND TARGET fdk-aac AND TARGET openaptx AND TARGET lc3))
    message(STATUS "a2dpwb_decode: skipped (needs sbc_decoder, fdk-aac, openaptx and lc3; "
        "run: git submodule update --init --recursive)")
    return()
endif()

add_executable(a2dpwb_decode a2dp_decode.cpp le_audio_decode.cpp)
target_link_libraries(a2dpwb_decode PRIVATE sbc_decoder fdk-aac openaptx lc3)
target_include_directories(a2dpwb_decode PRIVATE
    ${CMAKE_SOURCE_DIR}/extern/fdk-aac/libAACdec/include
    ${CMAKE_SOURCE_DIR}/extern/fdk-aac/libSYS/include
)

if(MSVC)
    target_compile_options(a2dpwb_decode PRIVATE /W3 /utf-8)
    target_compile_definitions(a2dpwb_decode PRIVATE _CRT_SECURE_NO_WARNINGS)
else()
    target_compile_options(a2dpwb_decode PRIVATE -Wall -Wextra)
endif()
