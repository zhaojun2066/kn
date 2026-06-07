#!/usr/bin/env python3
"""Hook: API 密钥扫描
Event: PreToolUse  Matcher: Write|Edit
扫描写入内容中是否包含 API key / token，命中则阻断"""
import sys, json, re

SECRET_PATTERNS = [
    (r'sk-ant-api\d{2}-[a-zA-Z0-9\-_]{90,}',   'Anthropic API Key'),
    (r'sk-[a-zA-Z0-9]{20,}T3BlbkFJ[a-zA-Z0-9]{20,}', 'OpenAI API Key'),
    (r'AKIA[0-9A-Z]{16}',                       'AWS Access Key'),
    (r'ghp_[a-zA-Z0-9]{36}',                    'GitHub Personal Access Token'),
    (r'glpat-[a-zA-Z0-9\-_]{20,}',              'GitLab PAT'),
    (r'eyJ[a-zA-Z0-9\-_]{20,}\.[a-zA-Z0-9\-_]{20,}\.[a-zA-Z0-9\-_]{20,}', 'JWT Token'),
    (r'-----BEGIN (RSA|EC|DSA|OPENSSH) PRIVATE KEY-----', 'Private Key'),
    (r'sk_live_[0-9a-zA-Z]{24,}',               'Stripe Live Key'),
    (r'xox[baprs]-[a-zA-Z0-9\-]{10,}',          'Slack Token'),
]

def main():
    try:
        payload = json.loads(sys.stdin.read())
    except Exception:
        sys.exit(0)

    # Check both file_path content and patch content
    content = (
        payload.get('tool_input', {}).get('content', '') or
        payload.get('tool_input', {}).get('new_string', '') or
        ''
    )
    file_path = payload.get('tool_input', {}).get('file_path', '')

    if not content and not file_path:
        sys.exit(0)

    text_to_scan = content + ' ' + file_path

    for pattern, name in SECRET_PATTERNS:
        if re.search(pattern, text_to_scan):
            print(f"[secret-scan] 检测到 {name}，已阻止写入。请使用环境变量代替。", file=sys.stderr)
            sys.exit(2)

    sys.exit(0)

if __name__ == '__main__':
    main()
