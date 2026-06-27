cask "ai-lemon-tools" do
  version "0.0.65"
  sha256 "47bd29ec734f059c263e02552191fb87f4d62bbbe20e651bc6a757d007acdfb2"

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
