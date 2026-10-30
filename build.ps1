param(
    # Preferred VBE mode. The bootloader falls back to smaller 16:9 and 4:3
    # modes when the firmware or video RAM cannot provide this size.
    [int]$GfxWidth = 1920,
    [int]$GfxHeight = 1080
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$Build = Join-Path $Root "build"
$Tools = Join-Path $Root "tools"

$Nasm = Join-Path $Tools "nasm-3.02\nasm.exe"
$Clang = Join-Path $Tools "llvm\bin\clang.exe"
$Cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
$Python = (Get-Command python).Source
$QemuImg = Join-Path $Tools "qemu\qemu-img.exe"

foreach ($tool in @($Nasm, $Clang, $Cargo, $Python)) {
    if (-not (Test-Path $tool)) {
        throw "Missing build tool: $tool"
    }
}

$env:Path = "$(Split-Path $Cargo);" + $env:Path
New-Item -ItemType Directory -Force -Path $Build | Out-Null

Write-Host "==> Building Rust commands" -ForegroundColor Cyan
$CommandsDir = Join-Path $Root "commands"
$CommandsTarget = Join-Path $CommandsDir "target\x86_64-unknown-none\release"
$CommandsLinker = Join-Path $CommandsDir "linker.ld"
$CommandNames = Get-ChildItem $CommandsDir -Directory |
    Where-Object {
        $_.Name -notin @("lib", "target", ".cargo") -and
        (Test-Path (Join-Path $_.FullName "Cargo.toml")) -and
        (Test-Path (Join-Path $_.FullName "desc.txt"))
    } |
    Select-Object -ExpandProperty Name

Push-Location $CommandsDir
try {
    foreach ($name in $CommandNames) {
        Write-Host "    $name"
        & $Cargo rustc -p $name --release --target x86_64-unknown-none -- `
            "-Clink-arg=-T$CommandsLinker" `
            "-Clink-arg=--gc-sections"
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to build command: $name"
        }
    }
}
finally {
    Pop-Location
}

Write-Host "==> Packing /commands archive" -ForegroundColor Cyan
& $Python (Join-Path $Tools "make_archive.py") `
    --commands-dir $CommandsDir `
    --target-dir $CommandsTarget `
    --out (Join-Path $Build "commands.hsb")
if ($LASTEXITCODE -ne 0) { throw "Archive build failed" }

Write-Host "==> Assembling kernel objects" -ForegroundColor Cyan
$KernelAsm = Join-Path $Root "src\kernel\asm"
$KernelCObj = Join-Path $Build "vga.o"
$StringObj = Join-Path $Build "string.o"
$EntryObj = Join-Path $Build "entry.o"
$InterruptsObj = Join-Path $Build "interrupts.o"
$RunObj = Join-Path $Build "run.o"
$BlobObj = Join-Path $Build "commands_blob.o"

& $Nasm -f elf64 "-I$Build/" -o $EntryObj (Join-Path $KernelAsm "entry.asm")
if ($LASTEXITCODE -ne 0) { throw "NASM failed: entry.asm" }
& $Nasm -f elf64 "-I$Build/" -o $InterruptsObj (Join-Path $KernelAsm "interrupts.asm")
if ($LASTEXITCODE -ne 0) { throw "NASM failed: interrupts.asm" }
& $Nasm -f elf64 "-I$Build/" -o $RunObj (Join-Path $KernelAsm "run.asm")
if ($LASTEXITCODE -ne 0) { throw "NASM failed: run.asm" }
& $Nasm -f elf64 "-I$Build/" -o $BlobObj (Join-Path $KernelAsm "commands_blob.asm")
if ($LASTEXITCODE -ne 0) { throw "NASM failed: commands_blob.asm" }

$CFlags = @(
    "--target=x86_64-unknown-none-elf",
    "-ffreestanding",
    "-fno-builtin",
    "-fno-stack-protector",
    "-fno-pic",
    "-mno-red-zone",
    "-mcmodel=kernel",
    "-fno-asynchronous-unwind-tables",
    "-fno-unwind-tables",
    "-O2",
    "-Wall",
    "-Wextra",
    "-c"
)
& $Clang @CFlags (Join-Path $Root "src\kernel\c\vga.c") -o $KernelCObj
if ($LASTEXITCODE -ne 0) { throw "vga.c compile failed" }
& $Clang @CFlags (Join-Path $Root "src\kernel\c\string.c") -o $StringObj
if ($LASTEXITCODE -ne 0) { throw "string.c compile failed" }

Write-Host "==> Building kernel" -ForegroundColor Cyan
$KernelDir = Join-Path $Root "src\kernel"
$KernelLinker = Join-Path $KernelDir "linker.ld"
$KernelElf = Join-Path $KernelDir "target\x86_64-unknown-none\release\hollowstar-kernel"
if (-not (Test-Path $KernelElf)) {
    $KernelElf = $KernelElf + ".exe"
}
# The command archive and asm/C objects are linked in as external objects;
# cargo cannot see that they changed, so drop the cached kernel artifacts to
# force a real recompile and relink every build.
$KernelDeps = Join-Path $KernelDir "target\x86_64-unknown-none\release\deps"
Get-ChildItem $KernelDeps -Filter "hollowstar_kernel-*" -ErrorAction SilentlyContinue | Remove-Item -Force
if (Test-Path $KernelElf) { Remove-Item $KernelElf -Force }
Push-Location $KernelDir
try {
    & $Cargo rustc --release --target x86_64-unknown-none -- `
        "-Clink-arg=-T$KernelLinker" `
        "-Clink-arg=$EntryObj" `
        "-Clink-arg=$InterruptsObj" `
        "-Clink-arg=$RunObj" `
        "-Clink-arg=$BlobObj" `
        "-Clink-arg=$KernelCObj" `
        "-Clink-arg=$StringObj" `
        "-Clink-arg=--gc-sections" `
        "-Clink-arg=--build-id=none"
    if ($LASTEXITCODE -ne 0) { throw "Kernel link failed" }
}
finally {
    Pop-Location
}

$KernelElf = Join-Path $KernelDir "target\x86_64-unknown-none\release\hollowstar-kernel"
if (-not (Test-Path $KernelElf)) {
    $KernelElf = $KernelElf + ".exe"
}
if (-not (Test-Path $KernelElf)) {
    throw "Kernel ELF not found"
}

Write-Host "==> Flattening kernel" -ForegroundColor Cyan
$KernelBin = Join-Path $Build "kernel.bin"
& $Python (Join-Path $Tools "elf_to_flat.py") $KernelElf $KernelBin 0x100000
if ($LASTEXITCODE -ne 0) { throw "Kernel flatten failed" }

Write-Host "==> Assembling bootloader" -ForegroundColor Cyan
$BootBin = Join-Path $Build "boot.bin"
$IsoBootBin = Join-Path $Build "iso_boot.bin"
$Stage2Bin = Join-Path $Build "stage2.bin"
$Stage2IsoBin = Join-Path $Build "stage2_iso.bin"
& $Nasm -f bin -o $BootBin (Join-Path $Root "src\bootloader\boot.asm")
& $Nasm -f bin -o $IsoBootBin (Join-Path $Root "src\bootloader\iso_boot.asm")
$GfxDefines = @("-DGFX_WIDTH=$GfxWidth", "-DGFX_HEIGHT=$GfxHeight")
& $Nasm -f bin @GfxDefines -o $Stage2Bin (Join-Path $Root "src\bootloader\stage2.asm")
$KernelSize = (Get-Item $KernelBin).Length
$KernelSectors = [math]::Ceiling($KernelSize / 512)
& $Nasm -f bin @GfxDefines "-DISO_BOOT" "-DKERNEL_SIZE=$KernelSize" "-DKERNEL_SECTORS=$KernelSectors" `
    -o $Stage2IsoBin (Join-Path $Root "src\bootloader\stage2.asm")
if ($LASTEXITCODE -ne 0) { throw "NASM failed: stage2.asm" }
if ((Get-Item $BootBin).Length -ne 512) { throw "boot.bin is not 512 bytes" }
if ((Get-Item $IsoBootBin).Length -ne 512) { throw "iso_boot.bin is not 512 bytes" }
if ((Get-Item $Stage2Bin).Length -gt (32 * 512)) { throw "stage2.bin is too large" }
if ((Get-Item $Stage2IsoBin).Length -gt (32 * 512)) { throw "stage2_iso.bin is too large" }

Write-Host "==> Building disk image" -ForegroundColor Cyan
$Image = Join-Path $Build "hollowstar.img"
& $Python (Join-Path $Tools "build_image.py") `
    --boot $BootBin `
    --stage2 $Stage2Bin `
    --kernel $KernelBin `
    --out $Image `
    --size 16M
if ($LASTEXITCODE -ne 0) { throw "Disk image build failed" }

Write-Host "==> Building ISO" -ForegroundColor Cyan
$Iso = Join-Path $Build "hollowstar.iso"
& $Python (Join-Path $Tools "build_iso.py") `
    --boot $IsoBootBin `
    --stage2 $Stage2IsoBin `
    --kernel $KernelBin `
    --out $Iso
if ($LASTEXITCODE -ne 0) { throw "ISO build failed" }

if (Test-Path $QemuImg) {
    Write-Host "==> Creating VMware VMDK" -ForegroundColor Cyan
    $Vmdk = Join-Path $Build "Hollowstar.vmdk"
    if (Test-Path $Vmdk) { Remove-Item $Vmdk -Force }
    & $QemuImg convert -f raw -O vmdk $Image $Vmdk
    if ($LASTEXITCODE -ne 0) { throw "VMDK conversion failed" }
}

$Vmx = @"
.encoding = "UTF-8"
config.version = "8"
virtualHW.version = "14"
guestOS = "other-64"
displayName = "Hollowstar"
memsize = "8192"
numvcpus = "1"
firmware = "bios"
ide0:0.present = "TRUE"
ide0:0.fileName = "Hollowstar.vmdk"
ide0:0.deviceType = "disk"
ide1:0.present = "TRUE"
ide1:0.fileName = "hollowstar.iso"
ide1:0.deviceType = "cdrom-image"
ide1:0.startConnected = "TRUE"
floppy0.present = "FALSE"
ethernet0.present = "FALSE"
sound.present = "FALSE"
usb.present = "FALSE"
tools.syncTime = "FALSE"
bios.bootDelay = "0"
bios.bootOrder = "cdrom,hdd"
"@
[IO.File]::WriteAllText((Join-Path $Build "Hollowstar.vmx"), $Vmx, (New-Object Text.UTF8Encoding($false)))

$ArchiveSize = (Get-Item (Join-Path $Build "commands.hsb")).Length
$IsoSize = (Get-Item $Iso).Length
Write-Host ""
Write-Host "Hollowstar build complete." -ForegroundColor Green
Write-Host "  ISO       : $Iso ($IsoSize bytes)"
Write-Host "  raw image : $Image"
Write-Host "  kernel    : $KernelSize bytes"
Write-Host "  commands  : $ArchiveSize bytes ($($CommandNames.Count) commands)"
Write-Host "  VMware    : $(Join-Path $Build 'Hollowstar.vmx')"
exit 0
