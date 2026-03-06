class Loggrep < Formula
  desc "A smarter log parser with color-coded severity, time filtering, regex matching, and stats"
  homepage "https://github.com/dyascj/loggrep-cli"
  url "https://github.com/dyascj/loggrep-cli/archive/refs/tags/v0.1.0.tar.gz"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "loggrep", shell_output("#{bin}/loggrep --version")
  end
end
