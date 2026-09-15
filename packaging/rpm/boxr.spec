Name:           boxr
Version:        %{version}
Release:        1%{?dist}
Summary:        Fast, lightweight OCI container engine and runtime in Rust
License:        Apache-2.0
URL:            https://github.com/kchaitanya863/kc-docker

%description
Boxr is a zero-dependency, rootless-by-default container engine, image
builder, compose orchestrator, and runtime written in pure Rust.

%install
mkdir -p %{buildroot}%{_bindir}
install -m 755 %{_sourcedir}/boxr %{buildroot}%{_bindir}/boxr

mkdir -p %{buildroot}%{_datadir}/bash-completion/completions
%{buildroot}%{_bindir}/boxr completion bash > %{buildroot}%{_datadir}/bash-completion/completions/boxr 2>/dev/null || true

mkdir -p %{buildroot}%{_datadir}/zsh/site-functions
%{buildroot}%{_bindir}/boxr completion zsh > %{buildroot}%{_datadir}/zsh/site-functions/_boxr 2>/dev/null || true

mkdir -p %{buildroot}%{_datadir}/fish/vendor_completions.d
%{buildroot}%{_bindir}/boxr completion fish > %{buildroot}%{_datadir}/fish/vendor_completions.d/boxr.fish 2>/dev/null || true

%files
%{_bindir}/boxr
%{_datadir}/bash-completion/completions/boxr
%{_datadir}/zsh/site-functions/_boxr
%{_datadir}/fish/vendor_completions.d/boxr.fish

%changelog
* Tue Sep 15 2026 Boxr Contributors <https://github.com/kchaitanya863/kc-docker> - %{version}-1
- Automated RPM package release
