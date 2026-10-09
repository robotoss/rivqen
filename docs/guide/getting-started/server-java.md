# Server: Java

This guide adds Rivqen to a Java server that renders HTML.

::: warning Planned API — not released
:::

## Requirements

- A supported Java LTS version (see [Support matrix](/guide/support-matrix)).
- Spring Boot, or any Jakarta Servlet container.

## Step 1 — Add the dependency

::: code-group

```kotlin [Gradle (Kotlin DSL)]
// Planned API — not released
dependencies {
    implementation("dev.rivqen:rivqen-spring-boot-starter:0.1.0")
}
```

```xml [Maven]
<!-- Planned API — not released -->
<dependency>
  <groupId>dev.rivqen</groupId>
  <artifactId>rivqen-spring-boot-starter</artifactId>
  <version>0.1.0</version>
</dependency>
```

:::

## Step 2 — Configure

1. Enable Rivqen for the paths that serve marked pages.

```yaml
# Planned API — not released
rivqen:
  # RQP is the default. Add rivqen-server-legacy only for migration.
  paths: ["/catalog/**", "/news/**"]
```

## Step 3 — Plain Servlet (without Spring)

1. Register `RivqenFilter` for the paths of marked pages.

```java
// Planned API — not released
FilterRegistration.Dynamic f = ctx.addFilter("rivqen", new RivqenFilter(RivqenConfig.defaults()));
f.addMappingForUrlPatterns(null, false, "/catalog/*");
```

## Step 4 — Set cache headers and check normal visitors

1. Add `Cache-Control: private` to personal pages.
2. Open the page in a desktop browser. Check that the HTML did not change.

## Next steps

- [Engineering: Java server SDK](/engineering/server/java)
