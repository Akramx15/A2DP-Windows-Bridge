// Choosing the profile for the big button. While a stream runs, choosing
// another profile streams that one (stream.start stops the running one);
// otherwise it only becomes the active profile. The options name what the
// profile is for, so two of them can be told apart.

import { useApp } from "../app/context";
import { t } from "../i18n";
import type { Profile } from "../ipc/types";
import { purposeLabel } from "../lib/profile";
import { Select } from "./controls";
import { currentProfile } from "./stream";

/** The option's text: the profile's name and what it is for (its kind of
 *  use when it has one, else the device) */
function optionLabel(p: Profile): string {
  const what = purposeLabel(p.purpose_kind, t) || p.device_name || p.device_address;
  return what ? `${p.name} · ${what}` : p.name;
}

export function ProfileSwitcher() {
  const app = useApp();
  const snap = () => app.core.state.snap!;
  const value = () => currentProfile(snap())?.name ?? "";
  const choose = (name: string) => {
    const running = snap().stream.state === "streaming" || snap().stream.state === "connecting";
    void app.run(() =>
      running ? app.core.request("stream.start", { profile: name }) : app.core.request("profile.set_active", { name }),
    );
  };
  return (
    <div class="profile-switcher">
      <Select
        label={t("profile.header")}
        value={value()}
        options={snap().profiles.map((p) => ({ value: p.name, label: optionLabel(p) }))}
        onChange={choose}
      />
    </div>
  );
}
