from pathlib import Path


def test_dev_script_re_signs_the_installed_agent_binary():
    script = Path(__file__).resolve().parents[1] / "dev.sh"
    source = script.read_text()

    install_index = source.index('cp "$AGENT_SRC" "$AGENT_DST"')
    sign_index = source.index('codesign --force --sign - "$AGENT_DST"')
    launchd_index = source.index('echo "[kn dev] Writing dev launchd plist')

    assert install_index < sign_index < launchd_index
