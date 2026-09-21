Name:           boxr
Version:        %{version}
Release:        1%{?dist}
Summary:        Fast, lightweight OCI container engine and runtime in Rust
License:        MIT
URL:            https://github.com/kchaitanya863/boxr

%description
Boxr is a zero-dependency, rootless-by-default container engine, image
builder, compose orchestrator, and runtime written in pure Rust.

%install
mkdir -p %{buildroot}%{_bindir}
install -m 755 %{_sourcedir}/boxr %{buildroot}%{_bindir}/boxr

mkdir -p %{buildroot}%{_datadir}/bash-completion/completions
mkdir -p %{buildroot}%{_datadir}/zsh/site-functions
mkdir -p %{buildroot}%{_datadir}/fish/vendor_completions.d

if [ -f %{_sourcedir}/boxr.bash ]; then
    install -m 644 %{_sourcedir}/boxr.bash %{buildroot}%{_datadir}/bash-completion/completions/boxr
elif %{buildroot}%{_bindir}/boxr --version >/dev/null 2>&1; then
    %{buildroot}%{_bindir}/boxr completion bash > %{buildroot}%{_datadir}/bash-completion/completions/boxr 2>/dev/null || true
else
    touch %{buildroot}%{_datadir}/bash-completion/completions/boxr
fi

if [ -f %{_sourcedir}/_boxr ]; then
    install -m 644 %{_sourcedir}/_boxr %{buildroot}%{_datadir}/zsh/site-functions/_boxr
elif %{buildroot}%{_bindir}/boxr --version >/dev/null 2>&1; then
    %{buildroot}%{_bindir}/boxr completion zsh > %{buildroot}%{_datadir}/zsh/site-functions/_boxr 2>/dev/null || true
else
    touch %{buildroot}%{_datadir}/zsh/site-functions/_boxr
fi

if [ -f %{_sourcedir}/boxr.fish ]; then
    install -m 644 %{_sourcedir}/boxr.fish %{buildroot}%{_datadir}/fish/vendor_completions.d/boxr.fish
elif %{buildroot}%{_bindir}/boxr --version >/dev/null 2>&1; then
    %{buildroot}%{_bindir}/boxr completion fish > %{buildroot}%{_datadir}/fish/vendor_completions.d/boxr.fish 2>/dev/null || true
else
    touch %{buildroot}%{_datadir}/fish/vendor_completions.d/boxr.fish
fi

%files
%{_bindir}/boxr
%{_datadir}/bash-completion/completions/boxr
%{_datadir}/zsh/site-functions/_boxr
%{_datadir}/fish/vendor_completions.d/boxr.fish

%changelog
* Tue Sep 15 2026 Boxr Contributors <https://github.com/kchaitanya863/boxr> - %{version}-1
- Automated RPM package release
