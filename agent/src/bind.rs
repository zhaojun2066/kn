//! `kn-agent bind` CLI 命令 — 设备绑定入口 + 绑定码展示。

use crate::config::AgentConfig;
use crate::device;
use crate::error::Result;
use tokio_util::sync::CancellationToken;

/// `kn-agent bind` 命令入口。
///
/// 流程：
/// 1. 调用 device::bind_init() 获取 6 位绑定码
/// 2. 在终端显示 ASCII 框，展示绑定码和主机名
/// 3. 轮询 device::bind_poll() 等待 iOS App 确认
/// 4. 成功后保存 device_token
pub async fn run_bind_command(config: AgentConfig) -> Result<()> {
    // ── Step 1: 请求绑定码 ──
    let (bind_code, expires_in, _confirm_url) =
        device::bind_init(&config.cloud_http_url, &config.machine_id).await?;

    // ── Step 2: 显示绑定框 ──
    display_bind_box(&bind_code, &config.hostname, expires_in);

    // ── Step 3: 注册 Ctrl+C 处理 ──
    let shutdown = CancellationToken::new();
    let shutdown_clone = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        shutdown_clone.cancel();
    });

    // ── Step 4: 轮询绑定结果 ──
    eprintln!("[kn-agent] 等待 iOS App 确认绑定...");
    let token =
        device::bind_poll(&config.cloud_http_url, &bind_code, expires_in, shutdown).await?;

    // ── Step 5: 保存 token ──
    device::save_device_token(&token)?;

    println!("\n设备绑定成功！");
    Ok(())
}

// ── Display helpers ───────────────────────────────────────────

/// 在终端输出 ASCII 绑定码展示框。
fn display_bind_box(bind_code: &str, hostname: &str, expires_in_secs: u64) {
    let mins = expires_in_secs / 60;
    let secs = expires_in_secs % 60;
    let validity = if secs == 0 {
        format!("{} 分钟", mins)
    } else {
        format!("{} 分 {} 秒", mins, secs)
    };

    let code_line = pad_box_line(&format!("绑定码: {}", bind_code));
    let host_line = pad_box_line(&format!("主机名: {}", hostname));
    let validity_line = pad_box_line(&format!("有效期: {}", validity));

    println!();
    println!("╔══════════════════════════════════╗");
    println!("{}", pad_box_line_center("📱 kn 设备绑定"));
    println!("{}", empty_box_line());
    println!("{}", code_line);
    println!("{}", host_line);
    println!("{}", empty_box_line());
    println!("{}", pad_box_line("请用 kn iOS App"));
    println!("{}", pad_box_line("输入以上绑定码完成绑定"));
    println!("{}", validity_line);
    println!("╚══════════════════════════════════╝");
    println!();
}

/// 内容左对齐 + 右侧空格填充到 34 列（内部宽度）。
fn pad_box_line(content: &str) -> String {
    let inner_width: usize = 34;
    let display_w = display_width(content);
    let right_pad = inner_width.saturating_sub(display_w);
    format!("║{}{}║", content, " ".repeat(right_pad))
}

/// 内容居中到 34 列（内部宽度）。
fn pad_box_line_center(content: &str) -> String {
    let inner_width: usize = 34;
    let display_w = display_width(content);
    let left_pad = inner_width.saturating_sub(display_w) / 2;
    let right_pad = inner_width.saturating_sub(display_w.saturating_add(left_pad));
    format!(
        "║{}{}{}║",
        " ".repeat(left_pad),
        content,
        " ".repeat(right_pad)
    )
}

fn empty_box_line() -> String {
    "║                                  ║".to_string()
}

/// 粗略估算终端列宽：ASCII 字符为 1，非 ASCII（CJK、emoji 等）为 2。
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| if c as u32 > 0x7F { 2 } else { 1 })
        .sum()
}
