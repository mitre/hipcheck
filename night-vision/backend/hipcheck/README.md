# Night Vision Hipcheck artifacts

This directory defines the immutable Hipcheck artifact bundle used by the Night
Vision MVP. It is deliberately separate from a developer's Hipcheck install.

## Pin

| Artifact | Pinned value |
| --- | --- |
| Upstream repository | `https://github.com/mitre/hipcheck` |
| Upstream commit | `06a3db9394742a58a7fb3412b677db25feb6f678` |
| Upstream version at that commit | `3.15.0` |
| MVP policy | `config/Hipcheck.kdl` |
| MVP plugin | `mitre/binary`, built locally from the same commit |

The commit is resolved before the image is built and verified by the
`hipcheck-builder` stage in `../Dockerfile`. The source's `Cargo.lock` is used
with `cargo build --locked`. The policy references a local plugin manifest with
an exact local version (`0.0.0`), so a run cannot download an upgraded plugin.
Update the commit, policy, and compatibility fixtures together in a reviewed
change.

## Artifact layout

The backend image installs this bundle at `/opt/night-vision/hipcheck`:

| Need | Image path | Source in this directory |
| --- | --- | --- |
| `hc` executable | `/usr/local/bin/hc` | built from the pin |
| Policy | `/opt/night-vision/hipcheck/config/Hipcheck.kdl` | `config/Hipcheck.kdl` |
| Exec configuration | `/opt/night-vision/hipcheck/config/Exec.kdl` | `config/Exec.kdl` |
| Plugin manifest | `/opt/night-vision/hipcheck/plugins/binary/local-release-plugin.kdl` | `plugins/binary/local-release-plugin.kdl` |
| Plugin executable | `/opt/night-vision/hipcheck/target/release/binary` | built from the pin |
| Writable cache | `/var/cache/night-vision/hipcheck` | runtime volume |
| Writable data/work area | `/var/lib/night-vision/hipcheck` | runtime volume |

The backend process should always invoke Hipcheck with the explicit policy,
exec, cache, and working directory shown below. It must not use `hc` found on
the developer `PATH`, `HC_CACHE`, the user's home directory, or an ambient
Hipcheck configuration.

```sh
cd /opt/night-vision/hipcheck
/usr/local/bin/hc \
  --policy /opt/night-vision/hipcheck/config/Hipcheck.kdl \
  --exec /opt/night-vision/hipcheck/config/Exec.kdl \
  --cache /var/cache/night-vision/hipcheck \
  check https://github.com/mitre/hipcheck
```

`/var/lib/night-vision/hipcheck` is reserved for Night Vision-managed temporary
input or retained evidence; it is never a source of policy or plugin artifacts.
The backend owns bounded retention for both writable directories.

## Local development

Build the backend image and run the same immutable artifact bundle rather than
installing Hipcheck locally:

```sh
docker build -f backend/Dockerfile -t nv-server:hipcheck-mvp backend
docker run --rm --entrypoint /usr/local/bin/hc nv-server:hipcheck-mvp \
  --policy /opt/night-vision/hipcheck/config/Hipcheck.kdl \
  --exec /opt/night-vision/hipcheck/config/Exec.kdl \
  --cache /var/cache/night-vision/hipcheck \
  ready
```

For an interactive analysis, keep the container's working directory at
`/opt/night-vision/hipcheck` (the image default) and replace `ready` with
`check <target>`. The current MVP policy does not require a GitHub token. Add
only plugin-specific credentials when a later pinned policy requires them.

To make a small convenience image whose entrypoint is that same embedded `hc`:

```sh
docker build -f backend/hipcheck/Dockerfile -t nv-hipcheck:mvp backend/hipcheck
docker run --rm nv-hipcheck:mvp
```

## Container deployment

`backend/Dockerfile` includes the immutable executable, policy, exec
configuration, manifest, and plugin executable. `docker-compose.yml` mounts
separate writable cache and data volumes while keeping the policy and plugin
paths in the read-only image. Deployments that use a different container system
must provide equivalent writable mounts at the two paths above and must not
mount over `/opt/night-vision/hipcheck`.
