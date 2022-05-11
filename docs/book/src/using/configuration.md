# Configuration

## Hipcheck software versions
* Hipcheck requires certain minimum versions of the software for Git version control in order to work properly. If your version is out of date, Hipcheck will notify you at runtime. The same is also true for npm/node software as well as eslint, as both are used by Hipcheck in the background.

## Hipcheck Configuration Order of Operations

Order of prescedence generally is based on Command Line Interface Guidelines at https://clig.dev/#configuration
1. commandline flags, e.g. `hc --config config/ check repo https://github.com/facebook/react`
2. sytem environment variables
3. a .env file in hipcheck root
4. xgd spec, crate::dirs

### Example commands
* `HC_GITHUB_TOKEN=examplegithubtokenvaluehere hc check repo --config config/ https://github.com/facebook/react`
* `hc check repo https://github.com/facebook/react`
* `hc check --home /path-to-home-location-here --config config/ repo https://github.com/facebook/react`
* `hc check --config config/ --home /Users/username-here/hipcheck-home repo https://github.com/facebook/react`
* `hc check repo https://github.com/facebook/react`
* `HC_LOG=debug,salsa=off hc check repo https://github.com/assimp/assimp`
* `HC_LOG=hc_data=debug,salsa=off hc check repo https://github.com/facebook/react`
* `hc check maven https://repo.maven.apache.org/maven2/com/fasterxml/jackson/core/jackson-databind/2.12.4/jackson-databind-2.12.4.pom`
* `hc check npm https://registry.npmjs.org/chalk/`
* `hc check npm https://registry.npmjs.org/lodash/`
* `hc check npm node-ipc@9.2.1`
* `hc check pypi https://pypi.org/project/urllib3/1.26.6`
* `hc check pypi urllib3@1.26.6`
* `hc check spdx path/to/doc.spdx`
* gets Git repo url for latest version of package if no version specified (npm and pypi)
  * `hc check pypi urllib3`
  * `hc check pypi Flask`
  * `hc check npm node-ipc`

## Current configuration environment variables
* These can be set as system environment variable, inside of a .env file in hipcheck checkout root, and/or passed in via the command line

### HC_HOME, e.g. /home/username-here/put-my-git-repo-cache-here
* Can be set with --home flag from command line
* Can be set with HC_HOME in system env or .env file
* If none are set, it defaults to the default platform cache directory

### HC_CONFIG, e.g. /home/username-here/hipcheck-git-root-here/config/
* Can be set with --config flag from command line
* Can be set with HC_CONFIG in system env or .env file
* If none are set, it defaults to the default platform config directory
* The path on the system must exist, the directory must already be created and the four config files
  in hipcheck-git-root/config must be copied to this folder

### HC_DATA, e.g. /home/username-here/hipcheck-data-files-here/
* Can be set with --data flag from command line
* Can be set with HC_DATA in system env or .env file
* If none are set, it defaults to the default platform data directory from rust crate::dirs
* The path on the system must exist, the directory must already be created, and the custom Hipcheck
  module-deps.js file must be copied to this folder
* module-deps.js can be obtained by running `npm install -g module-deps@6.2 --no-audit --no-fund`

### HC_GITHUB_TOKEN, e.g. ghj_WHTYHEOEPIAFAKJBGDYoalsnfdsfjsdnq1ejz
`HC_GITHUB_TOKEN=ghj_WHTYHEOEPIAFAKJBGDYoalsnfdsfjsdnq1ejz hc check repo https://github.com/facebook/react`
* This is a secure value so do not commit it
* This can currently be passed into the command line as an env in example above, but not with a flag
* This can be set as HC_GITHUB_TOKEN in system env or hipcheck Git root .env file
* Public Github tokens can be obtained from any Git user account when logged in github.com
* https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token

### HC_LOG, e.g. HC_LOG=debug,salsa=off
* Configures what should be logged
* Can be set from command line as env 
* Can be set with HC_LOG in system env or .env file

### HC_LOG_STYLE, e.g. HC_LOG_STYLE=always
* Configures the format of the log output
* Can be set with HC_LOG_STYLE in system env or .env file
* Log style is controlled with the `HC_LOG_STYLE` environment variable. The acceptable
values are `always`, `auto`, or `never`, and they control whether to try outputting color codes
with the log messages.
