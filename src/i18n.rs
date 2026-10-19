//! Minimal UI localization.
//!
//! The English UI text is itself the lookup key (gettext-style): each [`Locale`]
//! provides translations, and anything a locale doesn't translate falls back to
//! the English source. That means new locales can be filled in incrementally —
//! partial translations are fine and simply show English for the gaps.
//!
//! Adding a language is two steps: add a [`Locale`] variant (with its code in
//! [`Locale::from_code`] / [`Locale::code`] / [`Locale::display_name`]), and add a
//! lookup arm in [`tr`] that maps the English strings it translates.

/// The user-interface language. English is the source language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    English,
    Vietnamese,
}

impl Locale {
    /// Resolve a `:set language=<code>` value (case-insensitive) to a locale.
    pub fn from_code(s: &str) -> Option<Locale> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Locale::English),
            "vi" | "vn" | "vietnamese" => Some(Locale::Vietnamese),
            _ => None,
        }
    }

    /// The short code used by `:set language=` and shown by `:set language?`.
    pub fn code(self) -> &'static str {
        match self {
            Locale::English => "en",
            Locale::Vietnamese => "vi",
        }
    }

    /// The language's own name, for menus and help.
    pub fn display_name(self) -> &'static str {
        match self {
            Locale::English => "English",
            Locale::Vietnamese => "Tiếng Việt",
        }
    }

    /// Every selectable locale, in display order.
    pub fn all() -> &'static [Locale] {
        &[Locale::English, Locale::Vietnamese]
    }
}

/// Translate a UI string into `locale`. The English text is the key; a string a
/// locale does not translate is returned unchanged (English fallback).
pub fn tr(locale: Locale, s: &'static str) -> &'static str {
    match locale {
        Locale::English => s,
        Locale::Vietnamese => vi(s).unwrap_or(s),
    }
}

/// Vietnamese translations of rvim's UI strings. Returns `None` for anything not
/// (yet) translated so [`tr`] can fall back to English.
fn vi(s: &str) -> Option<&'static str> {
    Some(match s {
        // Menu-bar titles.
        "File" => "Tệp",
        "Buffers" => "Bộ đệm",
        "Edit" => "Sửa",
        "View" => "Xem",
        "Window" => "Cửa sổ",
        "Language" => "Ngôn ngữ",
        "Tools" => "Công cụ",
        "Help" => "Trợ giúp",
        // File menu.
        "Write" => "Ghi",
        "Write As…" => "Ghi thành…",
        "Write & Quit" => "Ghi và thoát",
        "Save & Quit" => "Lưu và thoát",
        "Quit" => "Thoát",
        "Quit Without Saving" => "Thoát không lưu",
        "Open File…" => "Mở tệp…",
        "Read File…" => "Đọc tệp…",
        "Source Script…" => "Chạy kịch bản…",
        // Buffers menu.
        "List Buffers" => "Liệt kê bộ đệm",
        "Next Buffer" => "Bộ đệm kế",
        "Previous Buffer" => "Bộ đệm trước",
        "Go To Buffer…" => "Đến bộ đệm…",
        "Close Buffer" => "Đóng bộ đệm",
        // Edit menu.
        "Substitute (line)…" => "Thay thế (dòng)…",
        "Substitute (file)…" => "Thay thế (tệp)…",
        "Sort Lines" => "Sắp xếp dòng",
        "Sort Descending" => "Sắp xếp giảm dần",
        "Sort Unique" => "Sắp xếp duy nhất",
        "Go To Line…" => "Đến dòng…",
        "Clear Search Highlight" => "Xóa nổi bật tìm kiếm",
        // View menu + submenus.
        "Theme" => "Giao diện",
        "Line Numbers" => "Số dòng",
        "Folding" => "Gấp dòng",
        "Options" => "Tùy chọn",
        "Absolute On" => "Tuyệt đối: Bật",
        "Absolute Off" => "Tuyệt đối: Tắt",
        "Relative On" => "Tương đối: Bật",
        "Relative Off" => "Tương đối: Tắt",
        "Method: Manual" => "Cách: Thủ công",
        "Method: Indent" => "Cách: Thụt lề",
        "Method: Marker" => "Cách: Dấu",
        "Method: Syntax" => "Cách: Cú pháp",
        "Fold Column On" => "Cột gấp: Bật",
        "Fold Column Off" => "Cột gấp: Tắt",
        "Mouse On" => "Chuột: Bật",
        "Mouse Off" => "Chuột: Tắt",
        "Search Highlight On" => "Nổi bật tìm kiếm: Bật",
        "Search Highlight Off" => "Nổi bật tìm kiếm: Tắt",
        "Auto-indent On" => "Tự thụt lề: Bật",
        "Auto-indent Off" => "Tự thụt lề: Tắt",
        "Smart-indent On" => "Thụt lề thông minh: Bật",
        "Smart-indent Off" => "Thụt lề thông minh: Tắt",
        "Show Whitespace On" => "Hiện khoảng trắng: Bật",
        "Show Whitespace Off" => "Hiện khoảng trắng: Tắt",
        "Cursor Line On" => "Dòng con trỏ: Bật",
        "Cursor Line Off" => "Dòng con trỏ: Tắt",
        // Window menu.
        "Split" => "Chia ngang",
        "Vertical Split" => "Chia dọc",
        "Split File…" => "Chia với tệp…",
        "Close Window" => "Đóng cửa sổ",
        "Only This Window" => "Chỉ cửa sổ này",
        // Language menu.
        "Set Filetype" => "Đặt loại tệp",
        "Plain Text" => "Văn bản thường",
        // Help menu.
        "Version" => "Phiên bản",
        // Status-line scroll indicator.
        "Top" => "Đầu",
        "Bot" => "Cuối",
        "All" => "Tất cả",
        // Common status / error messages.
        "paste" => "dán",
        "nopaste" => "không dán",
        "readonly" => "chỉ đọc",
        "noreadonly" => "không chỉ đọc",
        "only one buffer" => "chỉ có một bộ đệm",
        "E32: No file name" => "E32: Không có tên tệp",
        "E444: Cannot close last window" => "E444: Không thể đóng cửa sổ cuối",
        "E37: No write since last change (add ! to override)" => {
            "E37: Chưa ghi từ lần sửa trước (thêm ! để ghi đè)"
        }
        "E45: 'readonly' option is set (add ! to override)" => {
            "E45: tùy chọn 'readonly' đang bật (thêm ! để ghi đè)"
        }
        // Mode indicators.
        "-- INSERT --" => "-- CHÈN --",
        "-- REPLACE --" => "-- THAY THẾ --",
        "-- VISUAL --" => "-- TRỰC QUAN --",
        "-- VISUAL LINE --" => "-- TRỰC QUAN DÒNG --",
        "-- VISUAL BLOCK --" => "-- TRỰC QUAN KHỐI --",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_code_accepts_aliases() {
        assert_eq!(Locale::from_code("vi"), Some(Locale::Vietnamese));
        assert_eq!(Locale::from_code("Vietnamese"), Some(Locale::Vietnamese));
        assert_eq!(Locale::from_code("EN"), Some(Locale::English));
        assert_eq!(Locale::from_code("xx"), None);
    }

    #[test]
    fn tr_translates_or_falls_back() {
        // English is the identity.
        assert_eq!(tr(Locale::English, "File"), "File");
        // Vietnamese translates known strings…
        assert_eq!(tr(Locale::Vietnamese, "File"), "Tệp");
        assert_eq!(tr(Locale::Vietnamese, "-- INSERT --"), "-- CHÈN --");
        // …and falls back to English for anything untranslated.
        assert_eq!(tr(Locale::Vietnamese, "Not translated yet"), "Not translated yet");
        // Status words and messages are covered too.
        assert_eq!(tr(Locale::Vietnamese, "Top"), "Đầu");
        assert_eq!(
            tr(Locale::Vietnamese, "E444: Cannot close last window"),
            "E444: Không thể đóng cửa sổ cuối"
        );
    }
}
