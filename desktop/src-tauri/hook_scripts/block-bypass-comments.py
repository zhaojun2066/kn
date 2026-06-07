#!/usr/bin/env python3
"""Hook: 阻断绕过检查的注释
Event: PreToolUse  Matcher: Edit|Write
阻止 AI 使用 # noqa / @ts-ignore / eslint-disable 等绕过检查的注释

哲学: 让绕过检查的成本高于修复底层问题
"""
import sys, json, re

BYPASS_PATTERNS = [
    # Python
    (r'#\s*noqa\b',           '# noqa — 跳过 flake8/ruff 检查'),
    (r'#\s*type:\s*ignore',   '# type: ignore — 跳过 mypy 检查'),
    (r'#\s*pyright:\s*ignore', '# pyright: ignore — 跳过 Pyright 检查'),
    (r'#\s*pylint:\s*disable', '# pylint: disable — 跳过 Pylint 检查'),
    (r'#\s*bandit\s+skip',    '# bandit skip — 跳过安全扫描'),
    # TypeScript / JavaScript
    (r'//\s*@ts-ignore',       '// @ts-ignore — 跳过 TypeScript 检查'),
    (r'//\s*@ts-expect-error', '// @ts-expect-error — 预期类型错误'),
    (r'//\s*eslint-disable',   '// eslint-disable — 跳过 ESLint 检查'),
    (r'//\s*eslint-disable-next-line', '// eslint-disable-next-line — 跳过下一行 ESLint'),
    (r'/\*\s*eslint-disable',  '/* eslint-disable — 跳过 ESLint 检查（块级）'),
    # Testing
    (r'@pytest\.mark\.skip\b', '@pytest.mark.skip — 跳过测试'),
    (r'@unittest\.skip\b',     '@unittest.skip — 跳过测试'),
    (r'it\.skip\(',            'it.skip() — 跳过测试用例'),
    (r'test\.skip\(',          'test.skip() — 跳过测试'),
    (r'xtest\(',               'xtest() — 跳过测试'),
    (r'xit\(',                 'xit() — 跳过测试'),
    # Rust
    (r'#\[allow\(',            '#[allow(...)] — 抑制 Rust lint'),
    (r'#\[cfg\(test\)\]',      '#[cfg(test)] — 非测试代码中混入测试条件编译'),
]

# allow(cfg(test)) 在测试模块中是正常的，只在非测试文件中拦截
def is_in_test_file(file_path: str) -> bool:
    return '/tests/' in file_path or '/test/' in file_path or file_path.startswith('tests/')


def main():
    try:
        payload = json.loads(sys.stdin.read())
    except Exception:
        sys.exit(0)

    tool_input = payload.get('tool_input', {})

    # 提取写入内容
    content = (
        tool_input.get('content', '') or
        tool_input.get('new_string', '') or
        ''
    )
    file_path = tool_input.get('file_path', '')

    if not content:
        sys.exit(0)

    for pattern, name in BYPASS_PATTERNS:
        matches = re.findall(pattern, content)
        if matches:
            # cfg(test) 在测试文件中正常
            if 'cfg(test)' in name and is_in_test_file(file_path):
                continue

            print(
                f"[block-bypass-comments] 检测到绕过注释: {name}\n"
                f"  请修复底层问题而不是绕过检查。",
                file=sys.stderr,
            )
            sys.exit(2)

    sys.exit(0)


if __name__ == '__main__':
    main()
