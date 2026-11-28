# The Homebrew cask that installs Skald.app, as icebit-no/homebrew-tap carries it in Casks/skald.rb (D-14).
# packaging/macos/cask.sh writes a release's version, its zip's SHA-256 and the oldest macOS the bundle runs on in
# place of the placeholders; the release workflow commits the result to the tap (packaging/RELEASING.md).
cask "skald" do
  version "@VERSION@"
  sha256 "@SHA256@"

  url "https://github.com/icebit-no/skald/releases/download/v#{version}/Skald-#{version}.zip"
  name "Skald"
  desc "Norsk diktering på maskinen med talemodeller fra Nasjonalbiblioteket"
  homepage "https://skald.icebit.app/"

  # Skald ships for Apple Silicon only (D-34).
  depends_on arch: :arm64
  depends_on macos: :@MACOS@

  app "Skald.app"
  # The command line is the app's own executable (D-25), linked so it stays the same build after every upgrade.
  binary "#{appdir}/Skald.app/Contents/MacOS/skald"

  # Homebrew runs this on an upgrade too, so the old build is not left running from a bundle that is gone.
  uninstall quit: "app.icebit.skald"

  zap trash: "~/Library/Application Support/Skald"
end
