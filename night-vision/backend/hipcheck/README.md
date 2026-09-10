# Night Vision Hipcheck artifacts

This directory defines the Hipcheck artifact bundle used by the Night Vision
MVP. It is deliberately separate from a developer's Hipcheck install.

## Build source and provenance

| Artifact | Pinned value |
| --- | --- |
| Source repository | `https://github.com/mitre/hipcheck` |
| Build ref | `HIPCHECK_REF`, defaulting to `06a3db9394742a58a7fb3412b677db25feb6f678` |
| Resolved commit | `/opt/night-vision/hipcheck/REVISION` in the built image |
| MVP policy | `config/Hipcheck.kdl` |
| MVP plugin | `mitre/binary`, built locally from the same commit |

The Docker build fetches the selected ref with a read-only BuildKit secret,
checks out the resulting commit detached, and records its full SHA in
`REVISION`. The source's `Cargo.lock` is used with `cargo build --locked`.
The policy references a local plugin manifest with an exact local version
(`0.0.0`), so a run cannot download an upgraded plugin.

The default is the reviewed MVP commit used by local and CI builds. Upgrade it
only by changing `HIPCHECK_REF` to another full commit SHA and retaining the
image's `REVISION` file as provenance.

## Artifact layout

The backend image installs this bundle at `/opt/night-vision/hipcheck`:

| Need | Image path | Source in this directory |
| --- | --- | --- |
| `hc` executable | `/usr/local/bin/hc` | built from the pin |
| Policy | `/opt/night-vision/hipcheck/config/Hipcheck.kdl` | `config/Hipcheck.kdl` |
| Exec configuration | `/opt/night-vision/hipcheck/config/Exec.kdl` | `config/Exec.kdl` |
| Plugin manifest | `/opt/night-vision/hipcheck/plugins/binary/local-release-plugin.kdl` | `plugins/binary/local-release-plugin.kdl` |
| Plugin executable | `/opt/night-vision/hipcheck/target/release/binary` | built from the pin |
| Writable cache and working directory | `/var/cache/night-vision/hipcheck` | runtime volume |
| Writable data/work area | `/var/lib/night-vision/hipcheck` | runtime volume |

The backend process should always invoke Hipcheck with the explicit policy,
exec, cache, and working directory shown below. It must not use `hc` found on
the developer `PATH`, `HC_CACHE`, the user's home directory, or an ambient
Hipcheck configuration.

```sh
cd /var/cache/night-vision/hipcheck
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
docker build \

  --build-arg HIPCHECK_REF=06a3db9394742a58a7fb3412b677db25feb6f678 \
  --build-arg HIPCHECK_FETCH_EPOCH="$(date -u +%s)" \
  -f backend/Dockerfile -t nv-server:hipcheck-mvp backend
docker run --rm --entrypoint /usr/local/bin/hc nv-server:hipcheck-mvp \
  --policy /opt/night-vision/hipcheck/config/Hipcheck.kdl \
  --exec /opt/night-vision/hipcheck/config/Exec.kdl \
  --cache /var/cache/night-vision/hipcheck \
  ready
```

For an interactive analysis, keep the container's working directory at
`/var/cache/night-vision/hipcheck` (the image default) and replace `ready` with
`check <target>`. The current MVP policy does not require a GitHub token. Add
only plugin-specific credentials when a later pinned policy requires them.

To make a small convenience image whose entrypoint is that same embedded `hc`:

```sh
docker build -f backend/hipcheck/Dockerfile -t nv-hipcheck:mvp backend/hipcheck
docker run --rm nv-hipcheck:mvp
```

## Container deployment

`backend/Dockerfile` includes the executable, policy, exec configuration,
manifest, plugin executable, and resolved source revision. The GitLab token is
available only to the builder stage and is never copied into the image.
`docker-compose.yml` mounts separate writable cache and data volumes while
keeping the policy and plugin paths in the read-only image. Deployments that
use a different container system must provide equivalent writable mounts at the
two paths above and must not mount over `/opt/night-vision/hipcheck`.
