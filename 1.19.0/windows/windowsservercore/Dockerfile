FROM microsoft/windowsservercore

# $ProgressPreference: https://github.com/PowerShell/PowerShell/issues/2138#issuecomment-251261324
SHELL ["powershell", "-Command", "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue';"]

RUN $url = 'https://static.rust-lang.org/rustup/archive/1.5.0/x86_64-pc-windows-msvc/rustup-init.exe'; \
	Write-Host ('Downloading {0} ...' -f $url); \
	Invoke-WebRequest -Uri $url -OutFile 'go.zip'; \
	\
	$sha256 = '7500b705855c4f8e70e222ad1192bb46de15629f73a6459a98c9fd4bd06136bc'; \
	Write-Host ('Verifying sha256 ({0}) ...' -f $sha256); \
	if ((Get-FileHash rustup-init -Algorithm sha256).Hash -ne $sha256) { \
		Write-Host 'FAILED!'; \
		exit 1; \
	}; \
	\
	.\rustup-init -y --no-nodify-path --default-toolchain 1.19.0; \
	Remove-Item rustup-init-Force; \
        rustup --version; \
        cargo --version; \
        rustc --version; \
	Write-Host 'Complete.';
