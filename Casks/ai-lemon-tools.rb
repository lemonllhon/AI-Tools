cask "ai-lemon-tools" do
  version "0.0.64"
  sha256 "ac6d8f84fa87536532e07fd6b2139acdfccd6f18f9b9582c5f842d410303eec5"

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
    "~/Library/Application Support/com.jlcodes.ai-lemon-tools",
    "~/Library/Caches/com.jlcodes.ai-lemon-tools",
    "~/Library/Preferences/com.jlcodes.ai-lemon-tools.plist",
    "~/Library/Saved Application State/com.jlcodes.ai-lemon-tools.savedState",
  ]

  caveats <<~EOS
    The app is automatically quarantined by macOS. A postflight hook has been added to remove this quarantine.
    If you still encounter the "App is damaged" error, please run:
      sudo xattr -rd com.apple.quarantine "/Applications/AI Lemon Tools.app"
  EOS
end
