class Junkanoo < Formula
  desc "A Rust-based application"
  homepage "https://github.com/yourusername/junkanoo"
  url "https://github.com/yourusername/junkanoo/archive/refs/tags/v1.0.0.tar.gz"
  sha256 "" # You'll need to fill this in after creating the release
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--root", prefix, "--path", "."
  end

  test do
    system "#{bin}/junkanoo", "--version"
  end
end