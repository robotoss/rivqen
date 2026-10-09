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
cp "$LAB/node/app.js" "$WORK/node/"
(cd "$WORK/node" && echo '{"private":true}' > package.json && \
  npm install --no-fund --no-audit --ignore-scripts koa@2 through2@2 koa-is-json@1 sonic_differ@1.0.7)

# Java: upstream binary jar + Gson version from its POM + the page servlet.
cp "$UPSTREAM/sonic-java/lib/VasSonic-1.1.jar" "$WORK/java/WEB-INF/lib/"
curl -sSfL -o "$WORK/java/WEB-INF/lib/gson-2.8.2.jar" https://repo1.maven.org/maven2/com/google/code/gson/gson/2.8.2/gson-2.8.2.jar
cp "$LAB/java/web.xml" "$WORK/java/WEB-INF/web.xml"
docker run --rm -v "$LAB/java:/src:ro" -v "$WORK/java:/w" tomcat:9-jdk11 \
  javac -cp /usr/local/tomcat/lib/servlet-api.jar -d /w/WEB-INF/classes /src/PageServlet.java

docker rm -f lab-node lab-php lab-java >/dev/null 2>&1 || true
docker run -d --name lab-node -p 127.0.0.1:18081:8080 -v "$UPSTREAM:/upstream:ro" -v "$LAB/pages:/pages:ro" \
  -v "$WORK/node:/harness:ro" -e NODE_PATH=/harness/node_modules -w /harness node:22-alpine node app.js
docker run -d --name lab-php -p 127.0.0.1:18082:8000 -v "$UPSTREAM:/upstream:ro" -v "$LAB/pages:/pages:ro" \
  -v "$LAB/php:/harness:ro" php:8.3-cli-alpine php -S 0.0.0.0:8000 /harness/index.php
docker run -d --name lab-java -p 127.0.0.1:18083:8080 -v "$LAB/pages:/pages:ro" \
  -v "$WORK/java:/usr/local/tomcat/webapps/ROOT:ro" tomcat:9-jdk11
echo "Servers: node=18081 php=18082 java=18083 (localhost only)"
