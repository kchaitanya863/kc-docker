class Boxr < Formula
  desc "Fast, lightweight OCI container engine and runtime written in Rust"
  homepage "https://github.com/kchaitanya863/homebrew-tap"
  license "Apache-2.0"
  head "https://github.com/kchaitanya863/kc-docker.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/kchaitanya863/homebrew-tap/releases/download/v0.1.18/boxr-macos-arm64.tar.gz"
      sha256 "73ffad31df982763fb77506cbed08fa6728642e65ad244c1c10c4d2aba8b2966"
    else
      url "https://github.com/kchaitanya863/homebrew-tap/releases/download/v0.1.18/boxr-macos-x86_64.tar.gz"
      sha256 "926825b73c156a455e588e8768fb6e562a21d86330e9a69898931db63b47fa72"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/kchaitanya863/homebrew-tap/releases/download/v0.1.18/boxr-linux-arm64.tar.gz"
      sha256 "1554063cdcd474ded873d43c02b4b9f6f148d8dca00d389cf7463ea9aeea7695"
    else
      url "https://github.com/kchaitanya863/homebrew-tap/releases/download/v0.1.18/boxr-linux-x86_64.tar.gz"
      sha256 "b5f1a8ae904f43593f65a3a1752dfe40039ae696500e58efb1d1123192c886ac"
    end
  end

  def install
    if build.head?
      system "cargo", "install", *std_cargo_args
    else
      bin.install "bin/boxr"
    end

    # Shell completions
    bash_completion.install "completions/boxr.bash" => "boxr" if File.exist?("completions/boxr.bash")
    zsh_completion.install "completions/_boxr" => "_boxr" if File.exist?("completions/_boxr")
    fish_completion.install "completions/boxr.fish" => "boxr.fish" if File.exist?("completions/boxr.fish")
  end

  service do
    run [opt_bin/"boxr", "daemon"]
    keep_alive true
    log_path var/"log/boxr.log"
    error_log_path var/"log/boxr.log"
    working_dir var
  end

  def caveats
    <<~EOS
      To enable the docker drop-in alias wrapper:
        boxr alias --install
      or add to your shell profile:
        export PATH="$HOME/.boxr/bin:$PATH"
    EOS
  end

  test do
    assert_match "boxr", shell_output("#{bin}/boxr --version")
    assert_match "Containers:", shell_output("#{bin}/boxr info")
  end
end
