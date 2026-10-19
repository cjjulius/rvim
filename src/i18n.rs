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
    French,
}

impl Locale {
    /// Resolve a `:set language=<code>` value (case-insensitive) to a locale.
    pub fn from_code(s: &str) -> Option<Locale> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Locale::English),
            "vi" | "vn" | "vietnamese" => Some(Locale::Vietnamese),
            "fr" | "french" | "français" | "francais" => Some(Locale::French),
            _ => None,
        }
    }

    /// The short code used by `:set language=` and shown by `:set language?`.
    pub fn code(self) -> &'static str {
        match self {
            Locale::English => "en",
            Locale::Vietnamese => "vi",
            Locale::French => "fr",
        }
    }

    /// The language's own name, for menus and help.
    pub fn display_name(self) -> &'static str {
        match self {
            Locale::English => "English",
            Locale::Vietnamese => "Tiếng Việt",
            Locale::French => "Français",
        }
    }

    /// Every selectable locale, in display order.
    pub fn all() -> &'static [Locale] {
        &[Locale::English, Locale::Vietnamese, Locale::French]
    }
}

/// Translate a UI string into `locale`. The English text is the key; a string a
/// locale does not translate is returned unchanged (English fallback).
pub fn tr(locale: Locale, s: &'static str) -> &'static str {
    match locale {
        Locale::English => s,
        Locale::Vietnamese => vi(s).unwrap_or(s),
        Locale::French => fr(s).unwrap_or(s),
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
        // Templated messages (`{...}` placeholders are filled after translation).
        "\"{path}\" {lines} lines" => "\"{path}\" {lines} dòng",
        "\"{path}\" {lines} lines --reloaded--" => "\"{path}\" {lines} dòng --đã tải lại--",
        "\"{name}\" {bytes}B written" => "đã ghi \"{name}\" {bytes}B",
        "{count} buffer(s) done" => "{count} bộ đệm đã xong",
        "already editing \"{path}\"" => "đang mở \"{path}\"",
        "\"{path}\" (buffer switched)" => "\"{path}\" (đã chuyển bộ đệm)",
        "buffer closed; now \"{name}\"" => "đã đóng bộ đệm; giờ \"{name}\"",
        "E86: Buffer {n} does not exist" => "E86: Bộ đệm {n} không tồn tại",
        "E492: Not an editor command: {name}" => "E492: Không phải lệnh: {name}",
        // Mode indicators.
        "-- INSERT --" => "-- CHÈN --",
        "-- REPLACE --" => "-- THAY THẾ --",
        "-- VISUAL --" => "-- TRỰC QUAN --",
        "-- VISUAL LINE --" => "-- TRỰC QUAN DÒNG --",
        "-- VISUAL BLOCK --" => "-- TRỰC QUAN KHỐI --",
        _ => return None,
    })
}

/// French translations of rvim's UI strings. Returns `None` for untranslated
/// strings so [`tr`] can fall back to English.
fn fr(s: &str) -> Option<&'static str> {
    Some(match s {
        // Menu-bar titles.
        "File" => "Fichier",
        "Buffers" => "Tampons",
        "Edit" => "Édition",
        "View" => "Affichage",
        "Window" => "Fenêtre",
        "Language" => "Langue",
        "Tools" => "Outils",
        "Help" => "Aide",
        // File menu.
        "Write" => "Écrire",
        "Write As…" => "Écrire sous…",
        "Write & Quit" => "Écrire et quitter",
        "Save & Quit" => "Enregistrer et quitter",
        "Quit" => "Quitter",
        "Quit Without Saving" => "Quitter sans enregistrer",
        "Open File…" => "Ouvrir un fichier…",
        "Read File…" => "Lire un fichier…",
        "Source Script…" => "Exécuter un script…",
        // Buffers menu.
        "List Buffers" => "Lister les tampons",
        "Next Buffer" => "Tampon suivant",
        "Previous Buffer" => "Tampon précédent",
        "Go To Buffer…" => "Aller au tampon…",
        "Close Buffer" => "Fermer le tampon",
        // Edit menu.
        "Substitute (line)…" => "Substituer (ligne)…",
        "Substitute (file)…" => "Substituer (fichier)…",
        "Sort Lines" => "Trier les lignes",
        "Sort Descending" => "Trier (décroissant)",
        "Sort Unique" => "Trier (unique)",
        "Go To Line…" => "Aller à la ligne…",
        "Clear Search Highlight" => "Effacer la surbrillance",
        // View menu + submenus.
        "Theme" => "Thème",
        "Line Numbers" => "Numéros de ligne",
        "Folding" => "Repliage",
        "Options" => "Options",
        "Absolute On" => "Absolu : Activé",
        "Absolute Off" => "Absolu : Désactivé",
        "Relative On" => "Relatif : Activé",
        "Relative Off" => "Relatif : Désactivé",
        "Method: Manual" => "Méthode : Manuel",
        "Method: Indent" => "Méthode : Indentation",
        "Method: Marker" => "Méthode : Marqueur",
        "Method: Syntax" => "Méthode : Syntaxe",
        "Fold Column On" => "Colonne de repli : Activée",
        "Fold Column Off" => "Colonne de repli : Désactivée",
        "Mouse On" => "Souris : Activée",
        "Mouse Off" => "Souris : Désactivée",
        "Search Highlight On" => "Surbrillance : Activée",
        "Search Highlight Off" => "Surbrillance : Désactivée",
        "Auto-indent On" => "Auto-indentation : Activée",
        "Auto-indent Off" => "Auto-indentation : Désactivée",
        "Smart-indent On" => "Indentation intelligente : Activée",
        "Smart-indent Off" => "Indentation intelligente : Désactivée",
        "Show Whitespace On" => "Espaces visibles : Activé",
        "Show Whitespace Off" => "Espaces visibles : Désactivé",
        "Cursor Line On" => "Ligne du curseur : Activée",
        "Cursor Line Off" => "Ligne du curseur : Désactivée",
        // Window menu.
        "Split" => "Diviser",
        "Vertical Split" => "Diviser verticalement",
        "Split File…" => "Diviser avec un fichier…",
        "Close Window" => "Fermer la fenêtre",
        "Only This Window" => "Seulement cette fenêtre",
        // Language menu.
        "Set Filetype" => "Définir le type",
        "Plain Text" => "Texte brut",
        // Help menu.
        "Version" => "Version",
        // Status-line scroll indicator.
        "Top" => "Haut",
        "Bot" => "Bas",
        "All" => "Tout",
        // Common status / error messages.
        "paste" => "collage",
        "nopaste" => "sans collage",
        "readonly" => "lecture seule",
        "noreadonly" => "sans lecture seule",
        "only one buffer" => "un seul tampon",
        "E32: No file name" => "E32 : Aucun nom de fichier",
        "E444: Cannot close last window" => "E444 : Impossible de fermer la dernière fenêtre",
        "E37: No write since last change (add ! to override)" => {
            "E37 : Aucune écriture depuis la dernière modification (ajouter ! pour forcer)"
        }
        "E45: 'readonly' option is set (add ! to override)" => {
            "E45 : l'option 'readonly' est activée (ajouter ! pour forcer)"
        }
        // Templated messages (`{...}` placeholders are filled after translation).
        "\"{path}\" {lines} lines" => "\"{path}\" {lines} lignes",
        "\"{path}\" {lines} lines --reloaded--" => "\"{path}\" {lines} lignes --rechargé--",
        "\"{name}\" {bytes}B written" => "\"{name}\" {bytes} B écrit",
        "{count} buffer(s) done" => "{count} tampon(s) traité(s)",
        "already editing \"{path}\"" => "déjà ouvert \"{path}\"",
        "\"{path}\" (buffer switched)" => "\"{path}\" (tampon changé)",
        "buffer closed; now \"{name}\"" => "tampon fermé ; maintenant \"{name}\"",
        "E86: Buffer {n} does not exist" => "E86: le tampon {n} n'existe pas",
        "E492: Not an editor command: {name}" => "E492: commande inconnue : {name}",
        // Mode indicators.
        "-- INSERT --" => "-- INSERTION --",
        "-- REPLACE --" => "-- REMPLACEMENT --",
        "-- VISUAL --" => "-- VISUEL --",
        "-- VISUAL LINE --" => "-- VISUEL LIGNE --",
        "-- VISUAL BLOCK --" => "-- VISUEL BLOC --",
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
        assert_eq!(Locale::from_code("fr"), Some(Locale::French));
        assert_eq!(Locale::from_code("French"), Some(Locale::French));
        assert_eq!(Locale::from_code("xx"), None);
    }

    #[test]
    fn french_translates_with_english_fallback() {
        assert_eq!(tr(Locale::French, "File"), "Fichier");
        assert_eq!(tr(Locale::French, "-- INSERT --"), "-- INSERTION --");
        assert_eq!(tr(Locale::French, "All"), "Tout");
        // Untranslated strings fall back to English.
        assert_eq!(tr(Locale::French, "Not translated yet"), "Not translated yet");
    }

    #[test]
    fn buffer_and_command_messages_translate() {
        assert_eq!(
            tr(Locale::French, "E492: Not an editor command: {name}"),
            "E492: commande inconnue : {name}"
        );
        assert_eq!(
            tr(Locale::Vietnamese, "\"{path}\" (buffer switched)"),
            "\"{path}\" (đã chuyển bộ đệm)"
        );
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
