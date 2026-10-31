#!/usr/bin/env sh
# Start the upstream VasSonic servers (legacy protocol) in isolated containers.
# Usage: UPSTREAM=/path/to/VasSonic sh tools/upstream-lab/start.sh
# UPSTREAM must be a checkout of Tencent/VasSonic at 59936beff656d4b5718ff6444d6c5e001a2c5231.
set -eu
LAB="$(cd "$(dirname "$0")" && pwd)"
: "${UPSTREAM:?Set UPSTREAM to the VasSonic checkout}"
WORK="${WORK:-$LAB/.work}"
mkdir -p "$WORK/node" "$WORK/java/WEB-INF/lib" "$WORK/java/WEB-INF/classes"

# Node.js: dependencies of the upstream demo, install scripts disabled.
cp "$LAB/node/app.js" "$LAB/node/control.cjs" "$WORK/node/"
(cd "$WORK/node" && echo '{"private":true}' > package.json && \
  npm install --no-fund --no-audit --ignore-scripts koa@2 through2@2 koa-is-json@1 sonic_differ@1.0.7)

# Java: upstream binary jar + Gson version from its POM + the page servlet.
cp "$UPSTREAM/sonic-java/lib/VasSonic-1.1.jar" "$WORK/java/WEB-INF/lib/"
curl -sSfL -o "$WORK/java/WEB-INF/lib/gson-2.8.2.jar" https://repo1.maven.org/maven2/com/google/code/gson/gson/2.8.2/gson-2.8.2.jar
# Pinned (DL-083): SHA-256 of the jar whose SHA-1 equals the published gson-2.8.2.jar.sha1 of Maven Central
# (3edcfe49d2c6053a70a2a47e4e1c2f94998a49cf, checked 2026-10-31).
echo "b7134929f7cc7c04021ec1cc27ef63ab907e410cf0588e397b8851181eb91092  $WORK/java/WEB-INF/lib/gson-2.8.2.jar" | shasum -a 256 -c -
cp "$LAB/java/web.xml" "$WORK/java/WEB-INF/web.xml"
docker run --rm -v "$LAB/java:/src:ro" -v "$WORK/java:/w" tomcat:9-jdk11 \
  javac -cp /usr/local/tomcat/lib/servlet-api.jar:/w/WEB-INF/lib/gson-2.8.2.jar -d /w/WEB-INF/classes \
  /src/PageServlet.java /src/ControlFilter.java

# RIVQEN_CONFORMANCE=1 turns on the control endpoint (server/conformance/CONTRACT.md section 2);
# LAB_HOST_PORT is the published host port: the control Host check needs it (DL-081).
# Ports are published on 127.0.0.1 of the host only.
docker rm -f lab-node lab-php lab-java >/dev/null 2>&1 || true
docker run -d --name lab-node -p 127.0.0.1:18081:8080 -v "$UPSTREAM:/upstream:ro" -v "$LAB/pages:/pages:ro" \
  -v "$WORK/node:/harness:ro" -e NODE_PATH=/harness/node_modules -e RIVQEN_CONFORMANCE=1 -e LAB_HOST_PORT=18081 \
  -w /harness node:22-alpine node app.js
# post_max_size: a case body is up to 8 MiB; index.php refuses larger bodies itself (413).
docker run -d --name lab-php -p 127.0.0.1:18082:8000 -v "$UPSTREAM:/upstream:ro" -v "$LAB/pages:/pages:ro" \
  -v "$LAB/php:/harness:ro" -e RIVQEN_CONFORMANCE=1 -e LAB_HOST_PORT=18082 php:8.3-cli-alpine \
  php -d post_max_size=16M -d memory_limit=256M -S 0.0.0.0:8000 /harness/index.php
docker run -d --name lab-java -p 127.0.0.1:18083:8080 -v "$LAB/pages:/pages:ro" \
  -e RIVQEN_CONFORMANCE=1 -e LAB_HOST_PORT=18083 \
  -v "$WORK/java:/usr/local/tomcat/webapps/ROOT:ro" tomcat:9-jdk11
echo "Servers: node=18081 php=18082 java=18083 (localhost only)"
