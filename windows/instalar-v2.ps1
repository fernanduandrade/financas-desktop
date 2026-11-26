# Finanças 2 (Tauri): troca a versão Python pela nova, mantendo os dados.
$ErrorActionPreference = 'Stop'
$repo = 'https://github.com/fernanduandrade/financas-desktop/actions'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Write-Host '== Finanças 2: instalação ==' -ForegroundColor Green

# 1) acha o instalador: o .exe solto ou dentro do zip baixado do GitHub (financas-windows.zip)
$lugares = @($here, (Join-Path $here 'financas-tauri'), (Join-Path $env:USERPROFILE 'Downloads')) | Where-Object { Test-Path $_ }
$exe = Get-ChildItem -Path $lugares -Filter '*_x64-setup.exe' -File -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -like 'Finan*' } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $exe) {
  $zip = Get-ChildItem -Path $lugares -Filter 'financas-windows*.zip' -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
  if ($zip) {
    $tmp = Join-Path $env:TEMP 'financas-v2'
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive -Path $zip.FullName -DestinationPath $tmp -Force
    $exe = Get-ChildItem -Path $tmp -Filter '*_x64-setup.exe' -Recurse -File | Select-Object -First 1
  }
}
if (-not $exe) {
  Write-Host 'Não achei o instalador (Finanças_..._x64-setup.exe nem financas-windows.zip).' -ForegroundColor Yellow
  Write-Host "Baixe em $repo (última execução, em Artifacts: financas-windows), salve em Downloads e rode de novo."
  Start-Process $repo
  Read-Host 'Pressione Enter para fechar'
  exit 1
}
Write-Host "Instalador: $($exe.FullName)"

# 2) fecha e remove a versão Python (os dados ficam onde estão)
$data = Join-Path $env:APPDATA 'FinancasLocal'
Get-CimInstance Win32_Process -Filter "Name='pythonw.exe' OR Name='python.exe'" -ErrorAction SilentlyContinue |
  Where-Object { $_.CommandLine -like '*financas.pyw*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Get-Process -Name 'financas-local' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800
foreach ($folder in @([Environment]::GetFolderPath('Startup'), [Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))) {
  $lnk = Join-Path $folder 'Finanças.lnk'
  if (Test-Path $lnk) {
    # só apaga atalhos da versão Python (os da nova apontam para financas-local.exe)
    $alvo = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk).TargetPath
    if ($alvo -notlike '*financas-local.exe') { Remove-Item $lnk -Force -ErrorAction SilentlyContinue }
  }
}
Remove-Item (Join-Path $env:LOCALAPPDATA 'Programs\FinancasLocal') -Recurse -Force -ErrorAction SilentlyContinue

# 3) cópia de segurança do banco antes da troca
$db = Join-Path $data 'financas.db'
if (Test-Path $db) {
  $bk = Join-Path $data 'backups'
  New-Item -ItemType Directory -Force -Path $bk | Out-Null
  Copy-Item $db (Join-Path $bk ("antes-da-v2-{0:yyyy-MM-dd-HHmm}.db" -f (Get-Date))) -Force
  Write-Host "Dados mantidos em $data (com cópia de segurança em backups)."
}

# 4) instala a versão nova (sem perguntas) e abre
Write-Host 'Instalando...'
$p = Start-Process -FilePath $exe.FullName -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "O instalador terminou com erro ($($p.ExitCode))." }
$app = Get-ChildItem -Path $env:LOCALAPPDATA -Filter 'financas-local.exe' -Recurse -Depth 2 -File -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($app) { Start-Process $app.FullName }
Write-Host ''
Write-Host 'Pronto! O Finanças 2 está aberto e o ícone aparece perto do relógio.' -ForegroundColor Green
Write-Host 'Se o ícone estiver escondido na setinha ^, arraste-o para a barra.'
Read-Host 'Pressione Enter para fechar'
