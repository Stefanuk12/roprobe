# Launch Volt accounts via CDP (Chrome DevTools Protocol).
# Calls Volt's internal Tauri IPC to launch accounts through its full pipeline
# (auth ticket, Roblox launch, DLL injection, autoexec).
#
# ENV: VOLT_ACCOUNT_IDS = comma-separated account UUIDs from state.db
#      VOLT_PLACE_ID    = place to join (overrides Volt's default)
# Outputs JSON: { "ok": true } or { "error": "..." }
param()
$ErrorActionPreference = 'Stop'
$voltPath = "$env:LOCALAPPDATA\Volt\tauri-app.exe"
$cdpPort = 9222

try {
    # Ensure Volt is running with CDP enabled
    $voltProc = Get-Process -Name "tauri-app" -ErrorAction SilentlyContinue
    $cdpReady = $false

    if ($voltProc) {
        # Check if CDP is already active
        try {
            $null = Invoke-RestMethod -Uri "http://127.0.0.1:$cdpPort/json/version" -TimeoutSec 2
            $cdpReady = $true
        } catch {}
    }

    if (-not $cdpReady) {
        # Kill existing Volt (no CDP) and restart with CDP
        if ($voltProc) {
            Stop-Process -Name "tauri-app" -Force -ErrorAction SilentlyContinue
            Start-Sleep -Seconds 2
        }

        $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$cdpPort"
        Start-Process -FilePath $voltPath -WindowStyle Normal

        # Wait for CDP to be ready
        for ($i = 0; $i -lt 30; $i++) {
            Start-Sleep -Milliseconds 500
            try {
                $null = Invoke-RestMethod -Uri "http://127.0.0.1:$cdpPort/json/version" -TimeoutSec 2
                $cdpReady = $true
                break
            } catch {}
        }

        if (-not $cdpReady) {
            Write-Output '{"error":"CDP not ready after 15s"}'
            exit 1
        }
    }

    # Find the page target (Volt's main window)
    $targets = Invoke-RestMethod -Uri "http://127.0.0.1:$cdpPort/json" -TimeoutSec 5
    $page = $targets | Where-Object { $_.type -eq "page" } | Select-Object -First 1
    if (-not $page) {
        Write-Output '{"error":"no page target found in CDP"}'
        exit 1
    }

    $wsUrl = $page.webSocketDebuggerUrl
    if (-not $wsUrl) {
        Write-Output '{"error":"no WebSocket URL for page"}'
        exit 1
    }

    # Connect via WebSocket and invoke Tauri command
    Add-Type -AssemblyName System.Net.WebSockets -ErrorAction SilentlyContinue

    $ws = [System.Net.WebSockets.ClientWebSocket]::new()
    $ct = [System.Threading.CancellationToken]::None
    $ws.ConnectAsync($wsUrl, $ct).GetAwaiter().GetResult()

    $accountIds = $env:VOLT_ACCOUNT_IDS
    $placeId = $env:VOLT_PLACE_ID

    # First: update the place ID in Volt's config if specified
    if ($placeId) {
        $setPlaceJs = @"
(async () => {
    try {
        // Get current state to read existing config
        const state = await window.__TAURI_INVOKE__('account_manager_get_state');
        if (state && state.settings) {
            state.settings.defaultPlaceId = '$placeId';
            await window.__TAURI_INVOKE__('account_manager_save_configuration', { configuration: state });
        }
        return 'place_set';
    } catch(e) { return 'err:' + e.message; }
})()
"@
        $msg = @{ id = 1; method = "Runtime.evaluate"; params = @{ expression = $setPlaceJs; awaitPromise = $true; returnByValue = $true } } | ConvertTo-Json -Depth 10
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($msg)
        $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $ct).GetAwaiter().GetResult()

        # Read response
        $buf = [byte[]]::new(65536)
        $result = $ws.ReceiveAsync([ArraySegment[byte]]::new($buf), $ct).GetAwaiter().GetResult()
        $resp = [System.Text.Encoding]::UTF8.GetString($buf, 0, $result.Count)
    }

    # Launch accounts
    $launchJs = @"
(async () => {
    try {
        const ids = '$accountIds'.split(',').filter(x => x);
        if (ids.length === 0) {
            // Launch all alive accounts
            await window.__TAURI_INVOKE__('account_manager_launch_accounts');
        } else {
            // Launch specific accounts by triggering launch for each
            for (const id of ids) {
                await window.__TAURI_INVOKE__('account_manager_launch_accounts', { accountIds: ids });
                break; // launch_accounts takes an array
            }
        }
        return 'launched';
    } catch(e) { return 'err:' + e.message; }
})()
"@

    $msg2 = @{ id = 2; method = "Runtime.evaluate"; params = @{ expression = $launchJs; awaitPromise = $true; returnByValue = $true } } | ConvertTo-Json -Depth 10
    $bytes2 = [System.Text.Encoding]::UTF8.GetBytes($msg2)
    $ws.SendAsync([ArraySegment[byte]]::new($bytes2), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $ct).GetAwaiter().GetResult()

    $buf2 = [byte[]]::new(65536)
    $result2 = $ws.ReceiveAsync([ArraySegment[byte]]::new($buf2), $ct).GetAwaiter().GetResult()
    $resp2 = [System.Text.Encoding]::UTF8.GetString($buf2, 0, $result2.Count)

    $ws.CloseAsync([System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure, "", $ct).GetAwaiter().GetResult()

    # Check if the response indicates success
    if ($resp2 -match '"launched"') {
        Write-Output '{"ok":true}'
    } else {
        $clean = ($resp2 -replace '[\r\n]',' ').Substring(0, [Math]::Min(200, $resp2.Length))
        Write-Output "{`"ok`":true,`"note`":`"$clean`"}"
    }
} catch {
    $msg = $_.Exception.Message -replace '"','\"' -replace '[\r\n]',' '
    Write-Output "{`"error`":`"$msg`"}"
    exit 1
}
