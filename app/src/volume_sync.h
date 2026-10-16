/*
 * Volume Sync - one volume for Windows and the headphones
 *
 * While streaming, the Windows master volume of the output device that
 * A2DPWB follows and the headphones' AVRCP absolute volume are kept equal:
 * the volume keys and the Windows volume flyout change the headphones'
 * volume, and the headphones' own buttons and gestures move the Windows
 * slider. Loopback capture is taken before the master volume, so the audio
 * is attenuated once, on the headphones, and keeps its full resolution.
 *
 * Only the level is synced, not mute: muting the output device is how
 * the PC is kept silent while the headphones play.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef VOLUME_SYNC_H
#define VOLUME_SYNC_H

#include <cstdint>
#include <functional>
#include <string>
#include <utility>
#include <windows.h>

struct IMMDeviceEnumerator;
struct IAudioEndpointVolume;

/* Where the headphone volume lives: AVRCP absolute volume (A2DP, default)
 * or, for LE Audio, le_audio::Core (the VCP module). Levels 0-127. */
struct VolumeTarget {
    std::function<int()> remote_volume;        /* -1 = unknown */
    std::function<void(uint8_t)> request_volume;
    static VolumeTarget avrcp();
};

class VolumeSync {
public:
    /* device_id: the output device whose volume to sync, or empty for the
     * Windows default output device (followed when it changes). COM must be
     * initialized on the calling thread. */
    explicit VolumeSync(const std::wstring &device_id = L"", VolumeTarget target = VolumeTarget::avrcp());
    ~VolumeSync();

    VolumeSync(const VolumeSync &) = delete;
    VolumeSync &operator=(const VolumeSync &) = delete;

    /* Call periodically (a few times per second) while streaming. When the
     * headphones' volume first becomes known, Windows takes it over; after
     * that, whichever side changed is copied to the other. enabled=false
     * stops syncing and gives the device its previous volume back. */
    void tick(bool enabled);

    /* Stop syncing: the device gets the volume it had before */
    void stop();

    /* Test only (emulator runs): a simulated Windows volume (0-127) instead
     * of the output device's, so a test never changes the real one */
    struct Simulated {
        std::function<int()> get;
        std::function<void(int)> set;
    };
    void set_simulated(Simulated sim) { sim_ = std::move(sim); }

private:
    bool attach();
    void detach();
    int read_level() const;  /* 0-127, -1 = unknown */

    VolumeTarget target_;
    std::wstring fixed_id_;  /* empty = default output device */
    std::wstring id_;        /* device in use */
    IMMDeviceEnumerator *enumerator_ = nullptr;
    IAudioEndpointVolume *endpoint_ = nullptr;
    float original_ = -1.0f; /* the device's volume before syncing changed it */

    int last_windows_ = -1;  /* levels last seen in sync, 0-127 */
    int last_headphones_ = -1;
    DWORD hold_until_ = 0;   /* headphones settling after a Windows change */
    Simulated sim_;          /* test only */
};

#endif /* VOLUME_SYNC_H */
