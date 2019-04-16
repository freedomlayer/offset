echo $PATH

choco install capnproto strawberryperl

# Install yasm:
mkdir windows_build_tools

# Download yasm yasm:
curl -o windows_build_tools/yasm.exe https://www.tortall.net/projects/yasm/releases/yasm-1.3.0-win64.exe

# set PATH=windows_build_tools;%PATH%
