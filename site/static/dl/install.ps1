
# This installer delegates to the "real" installer included with each new
# release of Hipcheck.

$hc_version = "3.7.0"
$repo = "https://github.com/mitre/hipcheck"
$installer = "$repo/releases/download/hipcheck-v${hc_version}/hipcheck-installer.ps1"

irm "$installer" | iex "$Args"
