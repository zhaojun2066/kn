#!/usr/bin/env python3
"""Move a successful KN GitHub release-candidate artifact into kn-admin safely."""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import re
import ssl
import subprocess
import sys
import tempfile
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

VERSION_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
WORKFLOW = "build-desktop.yml"
WORKFLOW_NAME = "Build Desktop App"
MACOS_SYSTEM_CA_BUNDLE = Path("/etc/ssl/cert.pem")


def fail(message: str) -> None:
    raise RuntimeError(message)


def output(message: str) -> None:
    print(message, flush=True)


def command(args: list[str]) -> str:
    result = subprocess.run(args, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostic"
        fail(f"命令失败: {' '.join(args[:3])}… ({detail})")
    return result.stdout


def command_json(args: list[str]) -> Any:
    try:
        return json.loads(command(args))
    except json.JSONDecodeError as error:
        fail(f"GitHub CLI 返回了无效 JSON: {error}")


def config_path() -> Path:
    override = os.environ.get("KN_RELEASE_PUBLISH_CONFIG")
    if override:
        return Path(override).expanduser()
    return Path.home() / "workspace/me/miyao/admin/release-publish.json"


def load_config() -> dict[str, Any]:
    path = config_path()
    if not path.is_file():
        fail(f"找不到本机发布配置: {path}；请先按 references/setup.md 配置")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        fail(f"发布配置不是有效 JSON: {error}")
    if not isinstance(value, dict):
        fail("发布配置必须是 JSON 对象")
    for key in ("adminUrl", "adminEmail"):
        if not isinstance(value.get(key), str) or not value[key].strip():
            fail(f"发布配置缺少 {key}")
    parsed = urlparse(value["adminUrl"])
    if parsed.scheme != "https" or not parsed.netloc:
        fail("adminUrl 必须是完整 HTTPS 地址")
    value.setdefault("expectedBuildMinutes", 30)
    value.setdefault("pollSeconds", 30)
    value.setdefault("maxWaitMinutes", 90)
    value.setdefault("expectedMinProtocolVersion", 1)
    value.setdefault("keychainService", "kn.release-publish.admin")
    if not isinstance(value["expectedBuildMinutes"], int) or value["expectedBuildMinutes"] <= 0:
        fail("expectedBuildMinutes 必须是正整数")
    if not isinstance(value["pollSeconds"], int) or not 5 <= value["pollSeconds"] <= 300:
        fail("pollSeconds 必须在 5 到 300 秒之间")
    if not isinstance(value["maxWaitMinutes"], int) or value["maxWaitMinutes"] <= 0:
        fail("maxWaitMinutes 必须是正整数")
    if not isinstance(value["expectedMinProtocolVersion"], int) or value["expectedMinProtocolVersion"] <= 0:
        fail("expectedMinProtocolVersion 必须是正整数")
    return value


def github_repo(settings: dict[str, Any]) -> str:
    configured = settings.get("githubRepo")
    if isinstance(configured, str) and configured.strip():
        return configured.strip()
    value = command_json(["gh", "repo", "view", "--json", "nameWithOwner"])
    return str(value["nameWithOwner"])


def locate_run(repo: str, tag: str, requested_id: int | None) -> dict[str, Any]:
    fields = "databaseId,headBranch,status,conclusion,url,createdAt,updatedAt,workflowName"
    if requested_id:
        run = command_json(["gh", "run", "view", str(requested_id), "--repo", repo, "--json", fields])
        if run.get("headBranch") != tag or run.get("workflowName") != WORKFLOW_NAME:
            fail(f"指定 run 必须属于 {tag} 的 {WORKFLOW_NAME}")
        return run
    runs = command_json(["gh", "run", "list", "--repo", repo, "--workflow", WORKFLOW, "--limit", "100", "--json", fields])
    matches = [run for run in runs if run.get("headBranch") == tag and run.get("workflowName") == WORKFLOW_NAME]
    if not matches:
        fail(f"未找到 {tag} 的 {WORKFLOW} 工作流。请确认 tag 已推送，或稍后重试。")
    return max(matches, key=lambda run: run.get("createdAt", ""))


def elapsed_minutes(created_at: str) -> int:
    try:
        started = datetime.fromisoformat(created_at.replace("Z", "+00:00"))
        return max(0, int((datetime.now(timezone.utc) - started).total_seconds() // 60))
    except (TypeError, ValueError):
        return 0


def jobs_summary(repo: str, run_id: int) -> tuple[int, int, str]:
    info = command_json(["gh", "run", "view", str(run_id), "--repo", repo, "--json", "jobs"])
    jobs = info.get("jobs", [])
    finished = sum(1 for job in jobs if job.get("status") == "completed")
    labels = ", ".join(f"{job.get('name', 'job')}={job.get('conclusion') or job.get('status')}" for job in jobs)
    return finished, len(jobs), labels


def wait_for_completion(repo: str, run: dict[str, Any], settings: dict[str, Any]) -> dict[str, Any]:
    run_id = int(run["databaseId"])
    deadline = time.monotonic() + int(settings["maxWaitMinutes"]) * 60
    while True:
        current = command_json(["gh", "run", "view", str(run_id), "--repo", repo, "--json", "databaseId,headBranch,status,conclusion,url,createdAt,updatedAt"])
        completed, total, labels = jobs_summary(repo, run_id)
        elapsed = elapsed_minutes(current.get("createdAt", ""))
        remaining = max(0, int(settings["expectedBuildMinutes"]) - elapsed)
        output(f"[GitHub] {current.get('status')} | jobs {completed}/{total} | 已等待 {elapsed} 分钟 | 预计剩余约 {remaining} 分钟")
        if labels:
            output(f"[GitHub] {labels}")
        if current.get("status") == "completed":
            if current.get("conclusion") != "success":
                fail(f"GitHub Actions 未成功完成: {current.get('conclusion')}。查看: {current.get('url')}")
            output(f"[GitHub] 构建成功: {current.get('url')}")
            return current
        if time.monotonic() >= deadline:
            fail(f"等待 GitHub Actions 超过 {settings['maxWaitMinutes']} 分钟，已停止轮询。查看: {current.get('url')}")
        time.sleep(int(settings["pollSeconds"]))


def cargo_version() -> str:
    cargo = Path("Cargo.toml")
    if not cargo.is_file():
        fail("必须从仓库根目录运行，未找到 Cargo.toml")
    match = re.search(r'^version\s*=\s*"([^"]+)"\s*$', cargo.read_text(encoding="utf-8"), re.MULTILINE)
    if not match:
        fail("无法从根 Cargo.toml 读取 workspace version")
    return match.group(1)


def download_artifact(repo: str, run_id: int, version: str, directory: Path) -> Path:
    output("[GitHub] 正在下载 release candidate artifact…")
    command(["gh", "run", "download", str(run_id), "--repo", repo, "--name", f"release-candidate-v{version}", "--dir", str(directory)])
    return directory


def candidate_files(directory: Path) -> tuple[Path, Path, Path]:
    notes = list(directory.rglob("release-notes.md"))
    dmgs = list(directory.rglob("*.dmg"))
    arm = [file for file in dmgs if any(token in str(file).lower() for token in ("aarch64", "arm64", "apple-silicon"))]
    intel = [file for file in dmgs if any(token in str(file).lower() for token in ("x86_64", "x64", "intel"))]
    if len(notes) != 1 or not notes[0].read_text(encoding="utf-8").strip():
        fail("artifact 必须恰好包含一份非空 release-notes.md")
    if len(arm) != 1 or len(intel) != 1 or arm[0] == intel[0] or len(dmgs) != 2:
        fail("artifact 必须恰好包含一份 ARM DMG 与一份 Intel DMG")
    return arm[0], intel[0], notes[0]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def keychain_password(settings: dict[str, Any]) -> str:
    return command(["security", "find-generic-password", "-s", str(settings["keychainService"]), "-a", str(settings["adminEmail"]), "-w"]).rstrip("\n")


class AdminClient:
    def __init__(self, base_url: str, token: str | None = None):
        self.base = urlparse(base_url.rstrip("/"))
        self.token = token

    def _connection(self) -> http.client.HTTPSConnection:
        # Python.org's macOS framework may not include the current system trust
        # roots. Prefer the OS bundle that curl and system clients use, without
        # relaxing certificate or hostname verification.
        context = (
            ssl.create_default_context(cafile=str(MACOS_SYSTEM_CA_BUNDLE))
            if MACOS_SYSTEM_CA_BUNDLE.is_file()
            else ssl.create_default_context()
        )
        return http.client.HTTPSConnection(self.base.netloc, timeout=120, context=context)

    def _path(self, endpoint: str) -> str:
        prefix = self.base.path.rstrip("/")
        return f"{prefix}/api/admin/v1{endpoint}"

    def _response(self, connection: http.client.HTTPSConnection) -> Any:
        response = connection.getresponse()
        raw = response.read().decode("utf-8", errors="replace")
        try:
            body = json.loads(raw)
        except json.JSONDecodeError:
            fail(f"Admin 返回非 JSON（HTTP {response.status}）")
        if response.status < 200 or response.status >= 300 or body.get("code") != 0:
            fail(f"Admin 请求失败（HTTP {response.status}）: {body.get('message', '未知错误')}")
        return body.get("data")

    def json_request(self, endpoint: str, payload: dict[str, Any]) -> Any:
        connection = self._connection()
        headers = {"Accept": "application/json", "Content-Type": "application/json"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        connection.request("POST", self._path(endpoint), body=json.dumps(payload).encode(), headers=headers)
        return self._response(connection)

    def get(self, endpoint: str) -> Any:
        connection = self._connection()
        headers = {"Accept": "application/json"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        connection.request("GET", self._path(endpoint), headers=headers)
        return self._response(connection)

    def upload(self, fields: dict[str, str], arm: Path, intel: Path) -> Any:
        boundary = f"----kn-release-{uuid.uuid4().hex}"
        files = [("armDmg", arm), ("intelDmg", intel)]
        def text_part(name: str, value: str) -> bytes:
            return f"--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n".encode()
        def file_head(name: str, path: Path) -> bytes:
            return (f"--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{path.name}\"\r\nContent-Type: application/x-apple-diskimage\r\n\r\n").encode()
        length = sum(len(text_part(name, value)) for name, value in fields.items())
        length += sum(len(file_head(name, path)) + path.stat().st_size + 2 for name, path in files)
        length += len(f"--{boundary}--\r\n".encode())
        connection = self._connection()
        connection.putrequest("POST", self._path("/desktop-releases"))
        connection.putheader("Accept", "application/json")
        connection.putheader("Content-Type", f"multipart/form-data; boundary={boundary}")
        connection.putheader("Content-Length", str(length))
        if self.token:
            connection.putheader("Authorization", f"Bearer {self.token}")
        connection.endheaders()
        for name, value in fields.items():
            connection.send(text_part(name, value))
        total = sum(path.stat().st_size for _, path in files)
        sent = 0
        report_at = 0
        for name, path in files:
            connection.send(file_head(name, path))
            with path.open("rb") as handle:
                while chunk := handle.read(1024 * 1024):
                    connection.send(chunk)
                    sent += len(chunk)
                    percentage = int(sent * 100 / total)
                    if percentage >= report_at:
                        output(f"[Admin] 上传 {percentage}% ({sent // 1024 // 1024} / {total // 1024 // 1024} MiB)")
                        report_at += 10
            connection.send(b"\r\n")
        connection.send(f"--{boundary}--\r\n".encode())
        return self._response(connection)


def require_release(value: Any, version: str, expected_status: str, expected_min_protocol_version: int) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail("Admin 未返回发布记录")
    if value.get("version") != version or value.get("agentVersion") != version or value.get("status") != expected_status:
        fail("Admin 返回的发布记录与请求不一致")
    if value.get("minProtocolVersion") != expected_min_protocol_version:
        fail(f"Admin 返回的最低协议版本不是预期的 {expected_min_protocol_version}")
    if expected_status == "draft" and (not value.get("armSha256") or not value.get("intelSha256")):
        fail("Admin 未返回两份服务端 SHA-256")
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", help="release version, without the v prefix")
    parser.add_argument("--run-id", type=int, help="use a specific GitHub Actions run")
    parser.add_argument("--publish", action="store_true", help="publish after the new draft has been verified")
    parser.add_argument("--confirm-publish", help="must equal v<version> when --publish is used")
    args = parser.parse_args()
    if not VERSION_RE.fullmatch(args.version):
        fail("版本必须为 x.y.z，且不带 v 前缀")
    if args.publish and args.confirm_publish != f"v{args.version}":
        fail(f"公开发布需要 --confirm-publish v{args.version}")
    if cargo_version() != args.version:
        fail(f"根 Cargo.toml 版本为 {cargo_version()}，不等于请求的 {args.version}")
    settings = load_config()
    repo = github_repo(settings)
    tag = f"v{args.version}"
    output(f"[GitHub] 查找 {repo} 的 {tag} 构建…")
    run = wait_for_completion(repo, locate_run(repo, tag, args.run_id), settings)
    with tempfile.TemporaryDirectory(prefix=f"kn-release-{args.version}-") as temp:
        arm, intel, notes = candidate_files(download_artifact(repo, int(run["databaseId"]), args.version, Path(temp)))
        arm_hash, intel_hash = sha256(arm), sha256(intel)
        output(f"[Artifact] ARM: {arm.name}; Intel: {intel.name}; notes: {notes.stat().st_size} bytes")
        client = AdminClient(str(settings["adminUrl"]))
        token = client.json_request("/auth/login", {"email": settings["adminEmail"], "password": keychain_password(settings)})
        if not isinstance(token, dict) or not token.get("accessToken"):
            fail("Admin 登录未返回 access token")
        client.token = str(token["accessToken"])
        existing = client.get("/desktop-releases")
        draft = next((row for row in existing or [] if isinstance(row, dict) and row.get("version") == args.version), None)
        if draft is not None:
            if not args.publish:
                fail(f"Admin 已有 {args.version} 记录；为保护不可覆盖版本，已停止")
            draft = require_release(draft, args.version, "draft", settings["expectedMinProtocolVersion"])
            if draft["armSha256"] != arm_hash or draft["intelSha256"] != intel_hash:
                fail("现有 Admin 草稿的 SHA-256 与本次 GitHub artifact 不一致，已停止")
            output(f"[Admin] 使用已验收的草稿 #{draft['id']}；ARM SHA-256: {draft['armSha256']}; Intel SHA-256: {draft['intelSha256']}")
        else:
            draft = require_release(client.upload({"version": args.version, "agentVersion": args.version, "notes": notes.read_text(encoding="utf-8")}, arm, intel), args.version, "draft", settings["expectedMinProtocolVersion"])
            if draft["armSha256"] != arm_hash or draft["intelSha256"] != intel_hash:
                fail("Admin 返回的 SHA-256 与上传 artifact 不一致，草稿已保留待调查")
            output(f"[Admin] 草稿 #{draft['id']} 已创建；ARM SHA-256: {draft['armSha256']}; Intel SHA-256: {draft['intelSha256']}")
            if not args.publish:
                output(f"[Admin] 草稿保留待验收。确认后重新执行并加 --publish --confirm-publish v{args.version}。")
                return 0
        client.json_request(f"/desktop-releases/{draft['id']}/publish", {})
        published = next((row for row in client.get("/desktop-releases") if isinstance(row, dict) and row.get("id") == draft["id"]), None)
        require_release(published, args.version, "published", settings["expectedMinProtocolVersion"])
        output(f"[Admin] v{args.version} 已发布。")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(f"错误: {error}", file=sys.stderr)
        raise SystemExit(1)
