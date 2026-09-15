class Boxr < Formula
  desc "Fast, lightweight OCI container engine and runtime written in Rust"
  homepage "https://github.com/kchaitanya863/homebrew-boxr"
  license "Apache-2.0"
  head "https://github.com/kchaitanya863/kc-docker.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/kchaitanya863/homebrew-boxr/releases/download/v0.1.8/boxr-macos-arm64.tar.gz"
      sha256 "4c0251534adec8e6ce1c6edf604dc2ea6dc2e1876d7309e86a8cce2470ae3361"
    else
      url "https://github.com/kchaitanya863/homebrew-boxr/releases/download/v0.1.8/boxr-macos-x86_64.tar.gz"
      sha256 "09cba4337c73c5c70d3527670df7d7fad835282131e7101f186fe379292e06cc"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/kchaitanya863/homebrew-boxr/releases/download/v0.1.8/boxr-linux-arm64.tar.gz"
      sha256 "6f02eec53b609ac9f361aa865e6c7501af5b3a2f992343a28dd533a89542e9e5"
    else
      url "https://github.com/kchaitanya863/homebrew-boxr/releases/download/v0.1.8/boxr-linux-x86_64.tar.gz"
      sha256 "1ef4b94489b2c6618bcdb8e4b8ca48f6e193486fc7cb7207997054a99768027a"
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
