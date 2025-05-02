
# This installer delegates to the "real" installer included with each new
# release of Hipcheck.

$hc_version = "3.14.0"
$installer = "https://github.com/mitre/hipcheck/releases/download/hipcheck-v${hc_version}/hipcheck-installer.ps1"

irm "$installer" | iex
