class Junkanoo < Formula
  desc "Decentralized ephemeral file sharing CLI browser"
  homepage "https://github.com/maschad/junkanoo"
  url "https://github.com/maschad/junkanoo/archive/refs/tags/v1.0.0.tar.gz"
  sha256 "" # You'll need to fill this in after creating the release
  license "MIT"
  head "https://github.com/maschad/junkanoo.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    system "#{bin}/junkanoo", "--version"
  end
end