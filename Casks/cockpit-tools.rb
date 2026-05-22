cask "cockpit-tools" do
  version "0.0.13"
  sha256 "5725ce8e268c57c1e12bda06c67041e4364e7e650216a87e2dcefc3fc94e6e01"

  url "https://github.com/lemon-casino/ai-lemon-tools-release/releases/download/#{version}/AI.Lemon.Tools_#{version}_universal.dmg",
      verified: "https://github.com/lemon-casino/ai-lemon-tools-release/"
  name "AI Lemon Tools"
  desc "Codex-focused account manager"
  homepage "https://github.com/lemon-casino/ai-lemon-tools-release"

  auto_updates true

  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-cr", "#{appdir}/AI Lemon Tools.app"],
                   sudo: true
  end

  app "AI Lemon Tools.app"

  zap trash: [
    "~/Library/Application Support/com.jlcodes.cockpit-tools",
    "~/Library/Caches/com.jlcodes.cockpit-tools",
    "~/Library/Preferences/com.jlcodes.cockpit-tools.plist",
    "~/Library/Saved Application State/com.jlcodes.cockpit-tools.savedState",
  ]

  caveats <<~EOS
    The app is automatically quarantined by macOS. A postflight hook has been added to remove this quarantine.
    If you still encounter the "App is damaged" error, please run:
      sudo xattr -rd com.apple.quarantine "/Applications/AI Lemon Tools.app"
  EOS
end
