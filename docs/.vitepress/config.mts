import { defineConfig, type DefaultTheme } from 'vitepress'
import { withMermaid } from 'vitepress-plugin-mermaid'

// The site is published at https://robotoss.github.io/rivqen/.
// Override the base path with DOCS_BASE (for example "/" for a custom domain).
const base = process.env.DOCS_BASE ?? '/rivqen/'

// ---------------------------------------------------------------------------
// Layer 1 — Guide (people who use or evaluate Rivqen)
// ---------------------------------------------------------------------------
const guideSidebar: DefaultTheme.SidebarItem[] = [
  {
    text: 'Introduction',
    items: [
      { text: 'What is Rivqen?', link: '/guide/' },
      { text: 'How it works', link: '/guide/how-it-works' },
      { text: 'Project status', link: '/guide/status' },
      { text: 'Support matrix', link: '/guide/support-matrix' },
      { text: 'Performance', link: '/guide/performance' }
    ]
  },
  {
    text: 'Concepts',
    collapsed: false,
    items: [
      { text: 'Overview', link: '/guide/concepts/' },
      { text: 'Template and data', link: '/guide/concepts/template-and-data' },
      { text: 'Cache and revalidation', link: '/guide/concepts/cache-and-revalidation' },
      { text: 'Offline mode', link: '/guide/concepts/offline' },
      { text: 'Protocol modes', link: '/guide/concepts/protocol-modes' },
      { text: 'Transports', link: '/guide/concepts/transports' },
      { text: 'Security guarantees', link: '/guide/concepts/security' }
    ]
  },
  {
    text: 'Getting started',
    collapsed: false,
    items: [
      { text: 'Choose your path', link: '/guide/getting-started/' },
      { text: 'Android', link: '/guide/getting-started/android' },
      { text: 'iOS', link: '/guide/getting-started/ios' },
      { text: 'Web and React', link: '/guide/getting-started/web-react' },
      { text: 'Server: Node.js', link: '/guide/getting-started/server-node' },
      { text: 'Server: Java', link: '/guide/getting-started/server-java' },
      { text: 'Server: PHP', link: '/guide/getting-started/server-php' }
    ]
  },
  {
    text: 'Examples',
    collapsed: false,
    items: [
      { text: 'Overview', link: '/guide/examples/' },
      { text: 'Server (runs today)', link: '/guide/examples/server' },
      { text: 'Android', link: '/guide/examples/android' },
      { text: 'iOS', link: '/guide/examples/ios' },
      { text: 'Web', link: '/guide/examples/web' }
    ]
  },
  {
    text: 'More',
    items: [
      { text: 'Migrate from VasSonic', link: '/guide/migrate-from-vassonic' },
      { text: 'Troubleshooting', link: '/guide/troubleshooting' },
      { text: 'FAQ', link: '/guide/faq' },
      { text: 'Glossary', link: '/guide/glossary' }
    ]
  }
]

// ---------------------------------------------------------------------------
// Layer 2 — Engineering (people who build Rivqen)
// ---------------------------------------------------------------------------
const engineeringSidebar: DefaultTheme.SidebarItem[] = [
  {
    text: 'Start here',
    items: [
      { text: 'Engineering handbook', link: '/engineering/' },
      { text: 'Documentation style guide', link: '/engineering/style-guide' }
    ]
  },
  {
    text: 'Architecture',
    collapsed: false,
    items: [
      { text: 'System overview', link: '/engineering/architecture/' },
      { text: 'Invariants', link: '/engineering/architecture/invariants' },
      { text: 'Public surfaces', link: '/engineering/architecture/surfaces' },
      { text: 'Technology stack', link: '/engineering/architecture/technology-stack' },
      { text: 'Errors and logging', link: '/engineering/architecture/errors-logging' },
      { text: 'Decision records (ADR)', link: '/engineering/architecture/adr/' }
    ]
  },
  {
    text: 'Rust core',
    collapsed: true,
    items: [
      { text: 'Crate map', link: '/engineering/core/' },
      { text: 'Data model', link: '/engineering/core/data-model' },
      { text: 'Core API', link: '/engineering/core/api' },
      { text: 'Session state machine', link: '/engineering/core/session-fsm' },
      { text: 'Template engine', link: '/engineering/core/template-engine' },
      { text: 'Diff engine', link: '/engineering/core/diff-engine' },
      { text: 'Streaming', link: '/engineering/core/streaming' },
      { text: 'FFI boundary', link: '/engineering/core/ffi' },
      { text: 'Errors', link: '/engineering/core/errors' },
      { text: 'Resource limits', link: '/engineering/core/resource-limits' },
      { text: 'Dependencies', link: '/engineering/core/dependencies' }
    ]
  },
  {
    text: 'Protocol',
    collapsed: true,
    items: [
      { text: 'Overview', link: '/engineering/protocol/' },
      { text: 'RQP: block markup', link: '/engineering/protocol/markup' },
      { text: 'RQP markup: edge cases', link: '/engineering/protocol/markup-edge-cases' },
      { text: 'RQP: negotiation', link: '/engineering/protocol/negotiation' },
      { text: 'RQP: manifest and patch', link: '/engineering/protocol/manifest-patch' },
      { text: 'Versioning and legacy end of life', link: '/engineering/protocol/versioning' },
      { text: 'Golden fixtures', link: '/engineering/protocol/fixtures' },
      { text: 'Legacy: wire contract', link: '/engineering/protocol/legacy-wire' },
      { text: 'Legacy: markers grammar', link: '/engineering/protocol/legacy-markers' },
      { text: 'Legacy: client behavior', link: '/engineering/protocol/legacy-client' },
      { text: 'Legacy: divergences', link: '/engineering/protocol/legacy-divergences' },
      { text: 'Legacy: server traces', link: '/engineering/protocol/legacy-traces' }
    ]
  },
  {
    text: 'Transport',
    collapsed: true,
    items: [
      { text: 'Overview', link: '/engineering/transport/' },
      { text: 'HTTP stacks', link: '/engineering/transport/http' },
      { text: 'Realtime channels', link: '/engineering/transport/realtime' }
    ]
  },
  {
    text: 'Cache',
    collapsed: true,
    items: [
      { text: 'Cache layers', link: '/engineering/cache/' },
      { text: 'Cache identity', link: '/engineering/cache/identity' },
      { text: 'Freshness and invalidation', link: '/engineering/cache/policy' },
      { text: 'Storage and memory', link: '/engineering/cache/storage' }
    ]
  },
  {
    text: 'Platforms',
    collapsed: true,
    items: [
      { text: 'Android SDK', link: '/engineering/platforms/android' },
      { text: 'iOS SDK', link: '/engineering/platforms/ios' },
      { text: 'Web and React SDK', link: '/engineering/platforms/web-react' },
      { text: 'JS bridge contract', link: '/engineering/platforms/js-bridge' }
    ]
  },
  {
    text: 'Server SDKs',
    collapsed: true,
    items: [
      { text: 'Overview', link: '/engineering/server/' },
      { text: 'Node.js', link: '/engineering/server/node' },
      { text: 'Java', link: '/engineering/server/java' },
      { text: 'PHP', link: '/engineering/server/php' },
      { text: 'CDN and proxies', link: '/engineering/server/cdn' },
      { text: 'Conformance', link: '/engineering/server/conformance' }
    ]
  },
  {
    text: 'Security',
    collapsed: true,
    items: [
      { text: 'Overview', link: '/engineering/security/' },
      { text: 'Threat model', link: '/engineering/security/threat-model' },
      { text: 'Security controls', link: '/engineering/security/controls' },
      { text: 'Platform hardening', link: '/engineering/security/platform-hardening' },
      { text: 'Release profiles', link: '/engineering/security/release-profiles' },
      { text: 'Cryptography', link: '/engineering/security/crypto' },
      { text: 'Vulnerability management', link: '/engineering/security/vulnerability-management' }
    ]
  },
  {
    text: 'Quality',
    collapsed: true,
    items: [
      { text: 'Test strategy', link: '/engineering/quality/testing' },
      { text: 'Benchmarks', link: '/engineering/quality/benchmarks' },
      { text: 'Mutation testing', link: '/engineering/quality/mutation' },
      { text: 'Observability', link: '/engineering/quality/observability' }
    ]
  },
  {
    text: 'Delivery',
    collapsed: true,
    items: [
      { text: 'Repository layout', link: '/engineering/delivery/repository' },
      { text: 'CI/CD', link: '/engineering/delivery/ci-cd' },
      { text: 'Release and packaging', link: '/engineering/delivery/release' },
      { text: 'Documentation site', link: '/engineering/delivery/docs-site' }
    ]
  },
  {
    text: 'AI development',
    collapsed: true,
    items: [
      { text: 'Overview', link: '/engineering/ai/' },
      { text: 'Roles and routing', link: '/engineering/ai/roles' },
      { text: 'Sprint workflow', link: '/engineering/ai/sprint' },
      { text: 'Parallel work', link: '/engineering/ai/parallel' },
      { text: 'Decisions', link: '/engineering/ai/decisions' },
      { text: 'Documentation as you go', link: '/engineering/ai/documentation' }
    ]
  },
  {
    text: 'Coding standards',
    collapsed: true,
    items: [
      { text: 'Overview and architecture', link: '/engineering/standards/' },
      { text: 'Rust', link: '/engineering/standards/rust' },
      { text: 'Kotlin', link: '/engineering/standards/kotlin' },
      { text: 'Swift', link: '/engineering/standards/swift' },
      { text: 'TypeScript', link: '/engineering/standards/typescript' },
      { text: 'Java', link: '/engineering/standards/java' },
      { text: 'PHP', link: '/engineering/standards/php' }
    ]
  },
  {
    text: 'Plan and governance',
    collapsed: false,
    items: [
      { text: 'Roadmap and phases', link: '/engineering/plan/roadmap' },
      { text: 'Work packages (WP)', link: '/engineering/plan/work-packages' },
      { text: 'Gates and Definition of Done', link: '/engineering/plan/gates' },
      { text: 'Risk register', link: '/engineering/plan/risks' },
      { text: 'Open questions', link: '/engineering/plan/open-questions' },
      { text: 'Decision log', link: '/engineering/plan/decision-log' },
      { text: 'Sprint records', link: '/engineering/plan/sprints/' },
      { text: 'WP-17 S1', link: '/engineering/plan/sprints/WP-17-S1' },
      { text: 'Sprint record template', link: '/engineering/plan/sprints/template' }
    ]
  }
]

const researchSidebar: DefaultTheme.SidebarItem[] = [
  {
    text: 'Research',
    items: [
      { text: 'Overview', link: '/research/' },
      { text: 'Upstream: VasSonic audit', link: '/research/upstream-vassonic' },
      { text: 'Technology landscape', link: '/research/landscape' },
      { text: 'Standards', link: '/research/standards' },
      { text: 'Platform baselines (2026)', link: '/research/platform-baselines' },
      { text: 'Literature review', link: '/research/literature' },
      { text: 'Sources and confidence', link: '/research/sources' }
    ]
  }
]

const legalSidebar: DefaultTheme.SidebarItem[] = [
  {
    text: 'Legal',
    items: [
      { text: 'Overview', link: '/legal/' },
      { text: 'Licensing', link: '/legal/licensing' },
      { text: 'Third-party dependencies', link: '/legal/third-party' },
      { text: 'Clean-room provenance', link: '/legal/provenance' },
      { text: 'Upstream attribution', link: '/legal/upstream-attribution' },
      { text: 'Name and brand', link: '/legal/brand' }
    ]
  }
]

export default withMermaid(
  defineConfig({
    lang: 'en-US',
    title: 'Rivqen',
    description:
      'Rivqen is an independent open-source engine for fast first render, small updates and safe offline display of HTML pages in mobile and web apps.',
    base,
    cleanUrls: true,
    lastUpdated: true,
    // Engineering pages link to files that do not exist yet (planned code).
    // Keep the build strict for internal pages only.
    ignoreDeadLinks: [/^https?:\/\/localhost/],
    head: [
      ['meta', { name: 'theme-color', content: '#0e7c86' }],
      ['link', { rel: 'icon', type: 'image/svg+xml', href: `${base}logo.svg` }]
    ],
    vite: {
      // Mermaid is large by design; it is loaded only on pages with diagrams.
      build: { chunkSizeWarningLimit: 4096 }
    },
    markdown: {
      lineNumbers: false,
      theme: { light: 'github-light', dark: 'github-dark' }
    },
    themeConfig: {
      logo: '/logo.svg',
      siteTitle: 'Rivqen',
      nav: [
        { text: 'Guide', link: '/guide/', activeMatch: '^/guide/' },
        { text: 'Engineering', link: '/engineering/', activeMatch: '^/engineering/' },
        { text: 'Plan', link: '/engineering/plan/work-packages', activeMatch: '^/engineering/plan/' },
        { text: 'Research', link: '/research/', activeMatch: '^/research/' },
        { text: 'Legal', link: '/legal/', activeMatch: '^/legal/' }
      ],
      sidebar: {
        '/guide/': guideSidebar,
        '/engineering/': engineeringSidebar,
        '/research/': researchSidebar,
        '/legal/': legalSidebar
      },
      outline: { level: [2, 3], label: 'On this page' },
      search: { provider: 'local' },
      socialLinks: [{ icon: 'github', link: 'https://github.com/robotoss/rivqen' }],
      editLink: {
        pattern: 'https://github.com/robotoss/rivqen/edit/main/docs/:path',
        text: 'Edit this page on GitHub'
      },
      docFooter: { prev: 'Previous', next: 'Next' },
      footer: {
        message:
          'Apache-2.0 licensed. Independent project — not affiliated with, sponsored by, or endorsed by Tencent.',
        copyright: 'Copyright © 2026 The Rivqen Authors'
      }
    }
  }),
  {
    // Mermaid options. Diagrams use the site font and wrap long labels.
    mermaid: {
      securityLevel: 'strict',
      flowchart: { htmlLabels: true, wrappingWidth: 180, useMaxWidth: true },
      sequence: { useMaxWidth: true, wrap: true },
      state: { useMaxWidth: true }
    },
    mermaidPlugin: { class: 'mermaid rq-diagram' }
  }
)
