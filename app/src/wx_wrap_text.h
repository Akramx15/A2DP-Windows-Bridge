/*
 * Text wrapping that also breaks Chinese / Japanese text
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef WX_WRAP_TEXT_H
#define WX_WRAP_TEXT_H

#include <wx/arrstr.h>
#include <wx/stattext.h>
#include <wx/string.h>
#include <wx/window.h>

namespace wrap_detail {

inline bool is_cjk(wxUniChar c) {
    unsigned v = c.GetValue();
    return (v >= 0x2E80 && v <= 0x9FFF) || (v >= 0xF900 && v <= 0xFAFF) || (v >= 0xFF00 && v <= 0xFFEF);
}

/* Characters a line should not start / end with (simple kinsoku) */
inline bool no_line_start(wxUniChar c) {
    static const wxString chars = wxString::FromUTF8("、。，．）」』】〕！？ー…・：；%)]}.,!?:;");
    return chars.Find(c) != wxNOT_FOUND;
}
inline bool no_line_end(wxUniChar c) {
    static const wxString chars = wxString::FromUTF8("（「『【〔([{");
    return chars.Find(c) != wxNOT_FOUND;
}

} // namespace wrap_detail

/* Wraps at `width` pixels (in win's font) anywhere between CJK characters
 * and at spaces elsewhere (wxStaticText::Wrap() only breaks at spaces) */
inline wxString wrap_text(const wxWindow *win, const wxString &text, int width) {
    using namespace wrap_detail;
    wxString out;
    wxArrayString paragraphs = wxSplit(text, '\n', '\0');
    for (size_t p = 0; p < paragraphs.size(); p++) {
        if (p) out += '\n';
        const wxString &para = paragraphs[p];
        wxString line;
        int break_at = -1;  /* index in line after which it may break */
        for (size_t i = 0; i < para.length(); i++) {
            wxUniChar c = para[i];
            if (!line.empty()) {
                wxUniChar prev = line.Last();
                bool opportunity = prev == ' ' ||
                    ((is_cjk(prev) || is_cjk(c)) && !no_line_start(c) && !no_line_end(prev) && c != ' ');
                if (opportunity) break_at = static_cast<int>(line.length());
            }
            line += c;
            if (win->GetTextExtent(line).GetWidth() > width && break_at > 0) {
                wxString head = line.Left(break_at);
                head.Trim();
                out += head + '\n';
                line = line.Mid(break_at);
                line.Trim(false);
                break_at = -1;
            }
        }
        out += line;
    }
    return out;
}

/* Drop-in for wxStaticText::Wrap() */
inline void wrap_text(wxStaticText *text, int width) {
    text->SetLabel(wrap_text(text, text->GetLabel(), width));
}

#endif /* WX_WRAP_TEXT_H */
