cask "cockpit-tools" do
  version "0.0.38"
  sha256 "a3fd2e72d0c24c8069f2fee5fefd272fbdf57ca265ed799b98bd2e68b7bbf6eb"

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
