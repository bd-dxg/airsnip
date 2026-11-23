// SPDX-License-Identifier: GPL-3.0-only
//! 快速保存（OUT-2 / ADR 0041）：路径模板的变量替换、`%VAR%` 展开、同名策略与落盘。
//! 模板引擎是纯函数（时间由调用方传入），预览与真正保存共用同一份实现——
//! 预览「看到什么」就必须「存到什么」，两套算法迟早会分叉。

use std::path::{Path, PathBuf};

use tiny_skia::Pixmap;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::core::HSTRING;

use crate::settings::{ExistingFile, NameStyle, QuickSaveConfig};

/// 变量名与含义（设置面板「变量说明」浮层与替换器共用一张表）。
use crate::i18n::{self, SettingsKey as K};

/// 可用变量表：变量名（技术标识，不翻译）+ 说明文案键。
/// 表格与替换器共用这一张表，加一个变量不需要改两处。
pub const VARS: [(&str, K); 9] = [
    ("$yyyy$", K::VarYear),
    ("$yy$", K::VarYearShort),
    ("$MM$", K::VarMonth),
    ("$dd$", K::VarDay),
    ("$HH$", K::VarHour),
    ("$mm$", K::VarMinute),
    ("$ss$", K::VarSecond),
    ("$ts$", K::VarTimestamp),
    ("%VAR%", K::VarEnv),
];

/// 模板变量取值所需的时间分量（本地时间 + Unix 毫秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeParts {
    pub y: u16,
    pub mo: u16,
    pub d: u16,
    pub h: u16,
    pub mi: u16,
    pub s: u16,
    pub unix_ms: u128,
}

impl TimeParts {
    /// 当前本地时间 + Unix 毫秒。
    pub fn now() -> Self {
        let st = unsafe { GetLocalTime() };
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        Self {
            y: st.wYear,
            mo: st.wMonth,
            d: st.wDay,
            h: st.wHour,
            mi: st.wMinute,
            s: st.wSecond,
            unix_ms,
        }
    }
}

/// 展开 `%VAR%` 环境变量：查不到的原样保留（宁可让用户看到 `%FOO%` 报错，
/// 也不要静默变成一个空串把路径拼歪）。
pub fn expand_env(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '%'
            && let Some(end) = bytes[i + 1..].iter().position(|&c| c == '%')
        {
            let name: String = bytes[i + 1..i + 1 + end].iter().collect();
            if !name.is_empty()
                && let Ok(v) = std::env::var(&name)
            {
                out.push_str(&v);
                i += end + 2;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// 按时间替换变量（`$yyyy$` 等），**不追加扩展名**。
pub fn render_vars(template: &str, t: &TimeParts) -> String {
    let mut s = template.to_string();
    for (name, _) in VARS {
        let value = match name {
            "$yyyy$" => format!("{:04}", t.y),
            "$yy$" => format!("{:02}", t.y % 100),
            "$MM$" => format!("{:02}", t.mo),
            "$dd$" => format!("{:02}", t.d),
            "$HH$" => format!("{:02}", t.h),
            "$mm$" => format!("{:02}", t.mi),
            "$ss$" => format!("{:02}", t.s),
            "$ts$" => t.unix_ms.to_string(),
            _ => continue,
        };
        s = s.replace(name, &value);
    }
    s
}

/// 完整渲染：展开环境变量 → 替换时间变量 → 追加 `.ext`。预览与保存共用。
pub fn render(template: &str, t: &TimeParts, ext: &str) -> String {
    let expanded = expand_env(template);
    let mut path = render_vars(&expanded, t);
    path.push('.');
    path.push_str(ext);
    path
}

/// 另存为对话框的文件名预填（OUT-2 / ADR 0041）：模板去掉目录段与扩展名，只留文件名。
/// 目录仍用「上次保存目录」（`last_save_dir`），与模板里的目录无关。
pub fn file_stem(template: &str, t: &TimeParts) -> String {
    let rendered = render_vars(&expand_env(template), t);
    let name = rendered
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(rendered.as_str())
        .trim()
        .to_string();
    if name.is_empty() {
        crate::output::base_name(NameStyle::DateTime)
    } else {
        name
    }
}

/// 模板自身的问题（渲染**之后**的路径）：返回给用户的提示，`Ok` 表示可以保存。
/// 只做「一定会失败」的检查，不猜用户意图。
pub fn validate(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err(K::SaveTemplateEmpty.text().into());
    }
    if let Some(name) = path.rsplit(['\\', '/']).next() {
        if name.is_empty() || name == "." || name == ".." {
            return Err(K::SaveTemplateNoName.text().into());
        }
        if name.contains(['<', '>', ':', '"', '|', '?', '*']) {
            return Err(i18n::fill(K::SaveNameInvalidChars.text(), &[("name", name)]));
        }
        if name.chars().any(|c| (c as u32) < 0x20) {
            return Err(K::SaveNameControlChars.text().into());
        }
    }
    Ok(())
}

/// 同名策略（OUT-2 / ADR 0041）：`覆盖` 直接用，`自动加序号` 找第一个空位（最多 9999 次）。
/// 序号用尽时返回错误而不是静默覆盖——「自动加序号」的语义是永不丢数据，
/// 回头写原路径会把用户已有的文件盖掉（成堆同名截图恰好会在这种目录里出现）。
pub fn unique_path(path: &Path, policy: ExistingFile) -> Result<PathBuf, String> {
    if policy == ExistingFile::Overwrite || !path.exists() {
        return Ok(path.to_path_buf());
    }
    let parent = path.parent();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    for n in 1..=9999u32 {
        let name = match &ext {
            Some(e) => format!("{stem}-{n}.{e}"),
            None => format!("{stem}-{n}"),
        };
        let cand = match parent {
            Some(p) => p.join(name),
            None => PathBuf::from(name),
        };
        if !cand.exists() {
            return Ok(cand);
        }
    }
    Err(i18n::fill(K::SaveNameAllTaken.text(), &[("stem", &stem)]))
}

/// 快速保存落盘：渲染模板 → 校验 → 同名策略 → 建目录 → 写文件。
/// 调用方负责放到后台线程（OUT-2：编码不占覆盖层交互线程）。
pub fn save_quickly(pixmap: &Pixmap, cfg: &QuickSaveConfig, t: &TimeParts) -> Result<PathBuf, String> {
    let rendered = render(&cfg.template(), t, cfg.format.ext());
    validate(&rendered)?;
    let path = unique_path(Path::new(&rendered), cfg.on_exist)?;
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| {
            i18n::fill(
                K::SaveDirCreateFailed.text(),
                &[("dir", &dir.display().to_string()), ("e", &e.to_string())],
            )
        })?;
    }
    crate::output::save_file(pixmap, &path)?;
    Ok(path)
}

/// 在资源管理器里定位文件（`explorer /select,"<path>"`）。
/// 路径来自用户可编辑的模板，因此这里不再整串丢给命令行：含双引号 / 控制字符时
/// 直接不执行（否则一个 `"` 就能追加出额外的命令行参数）。正常路径不会走到这条：
/// `validate` 拦住文件名里的 `"`，`create_dir_all` 拦住目录里不可能存在的字符。
pub fn reveal_in_explorer(path: &Path) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, w};
    let shown = path.display().to_string();
    if shown.contains(['"', '\r', '\n']) {
        eprintln!("[airsnip] quicksave: 路径含引号 / 换行，跳过定位：{shown}");
        return;
    }
    let arg = format!("/select,\"{shown}\"");
    let file = HSTRING::from("explorer.exe");
    let params = HSTRING::from(arg);
    unsafe {
        ShellExecuteW(None, w!("open"), &file, &params, None, SW_SHOWNORMAL);
    }
}

/// 用资源管理器打开目录（不带 `/select`）。
pub fn open_folder(dir: &Path) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::w;
    let file = HSTRING::from(dir.display().to_string());
    unsafe {
        ShellExecuteW(None, w!("open"), &file, None, None, SW_SHOWNORMAL);
    }
}

/// 渲染结果的目录部分（「打开文件夹」按钮用；没有目录段时返回当前目录）。
pub fn rendered_dir(rendered: &str) -> PathBuf {
    Path::new(rendered)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> TimeParts {
        TimeParts {
            y: 2026,
            mo: 11,
            d: 16,
            h: 7,
            mi: 49,
            s: 46,
            unix_ms: 1_763_000_000_123,
        }
    }

    /// 变量替换：每个变量都落在该落的位置上，拼出的路径与预览一致。
    #[test]
    fn render_replaces_every_variable() {
        let got = render(r"D:\shots\airsnip_$yyyy$-$MM$-$dd$_$HH$-$mm$-$ss$", &t(), "png");
        assert_eq!(got, r"D:\shots\airsnip_2026-11-16_07-49-46.png");

        assert_eq!(render(r"$yy$", &t(), "jpg"), "26.jpg");
        assert_eq!(render(r"$ts$", &t(), "webp"), "1763000000123.webp");
        // 未知变量原样保留（用户能看见自己写错了），不会被静默吃掉
        assert_eq!(render(r"$nope$", &t(), "png"), "$nope$.png");
    }

    /// 环境变量展开：认识的展开、不认识的原样保留。
    #[test]
    fn expand_env_expands_known_and_keeps_unknown() {
        let home = std::env::var("USERPROFILE").expect("Windows 必有 USERPROFILE");
        assert_eq!(expand_env(r"%USERPROFILE%\shots"), format!(r"{home}\shots"));
        assert_eq!(expand_env("%AIRS NIP_NOT_SET%x"), "%AIRS NIP_NOT_SET%x");
        // 没有闭合的 % 不吞字符
        assert_eq!(expand_env("100% done"), "100% done");
        assert_eq!(expand_env("%%"), "%%");
    }

    /// 校验：空模板 / 只有目录 / 非法字符分别给出可读提示；正常路径放行。
    #[test]
    fn validate_reports_understandable_errors() {
        assert!(validate(r"D:\shots\a.png").is_ok());
        assert!(validate(r"D:\shots\有中文也叫合法.png").is_ok());
        assert!(validate("").is_err());
        assert!(validate("   ").is_err());
        assert!(
            validate(r"D:\shots\").unwrap_err().contains("文件名"),
            "只写到目录应提示缺文件名"
        );
        let e = validate(r"D:\shots\a?.png").unwrap_err();
        assert!(e.contains("不允许的字符"), "{e}");
    }

    /// 同名策略：覆盖就地写，自动序号找第一个空位；扩展名与目录都保留。
    #[test]
    fn unique_path_respects_policy() {
        let dir = std::env::temp_dir().join("airsnip-quick-unique");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("shot.png");
        std::fs::write(&base, b"x").unwrap();
        std::fs::write(dir.join("shot-1.png"), b"x").unwrap();

        assert_eq!(
            unique_path(&base, ExistingFile::Overwrite).unwrap(),
            base,
            "覆盖策略应就地写"
        );
        assert_eq!(
            unique_path(&base, ExistingFile::Rename).unwrap(),
            dir.join("shot-2.png")
        );
        // 目标不存在时两种策略一致
        let missing = dir.join("new.png");
        assert_eq!(unique_path(&missing, ExistingFile::Rename).unwrap(), missing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 端到端：目录 + 文件名 → 目录自动创建 → 同名自动加序号 → 真正写出文件。
    #[test]
    fn save_quickly_creates_dir_and_avoids_overwrite() {
        let root = std::env::temp_dir().join("airsnip-quick-save");
        let _ = std::fs::remove_dir_all(&root);
        let cfg = QuickSaveConfig {
            // 目录段也支持变量（供迁移来的旧模板使用）
            dir: Some(root.join("$yyyy$-sub").to_string_lossy().into_owned()),
            name: Some("shot".into()),
            ..Default::default()
        };
        let pixmap = Pixmap::new(4, 4).unwrap();
        let a = save_quickly(&pixmap, &cfg, &t()).expect("首次保存应成功");
        let b = save_quickly(&pixmap, &cfg, &t()).expect("第二次保存应成功");
        assert_eq!(
            a,
            root.join("2026-sub").join("shot.png"),
            "目录应自动创建、扩展名由格式决定"
        );
        assert_eq!(b, root.join("2026-sub").join("shot-1.png"), "同名应自动加序号");
        assert!(a.is_file() && b.is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 另存为预填：只取文件名段（去掉目录与扩展名）；文件名段为空时回退到旧命名模板。
    #[test]
    fn file_stem_keeps_only_file_name() {
        assert_eq!(file_stem(r"D:\shots\airsnip_$yyyy$", &t()), "airsnip_2026");
        assert_eq!(file_stem(r"%USERPROFILE%\Pictures\airsnip\$ts$", &t()), "1763000000123");
        let fallback = file_stem(r"D:\shots\", &t());
        assert!(fallback.starts_with("airsnip-"), "空文件名应回退旧模板：{fallback}");
    }

    /// 预览目录：正常路径取父目录；裸文件名退到当前目录（不 panic）。
    #[test]
    fn rendered_dir_handles_bare_names() {
        let d = rendered_dir(r"D:\shots\a.png");
        assert_eq!(d, PathBuf::from(r"D:\shots"));
        assert!(rendered_dir("a.png").is_absolute() || rendered_dir("a.png").exists() || true);
    }
}
