/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import type { Config } from '@docusaurus/types';
import { themes as prismThemes } from 'prism-react-renderer';
import { fbContent } from 'docusaurus-plugin-internaldocs-fb/internal';
import * as webpack from 'webpack';
import MonacoWebpackPlugin from 'monaco-editor-webpack-plugin';
import StylexPlugin from '@stylexjs/webpack-plugin';
import PyodidePlugin from '@pyodide/webpack-plugin';

function getNavBarItems() {
    return [
        {
            to: 'en/docs/',
            activeBasePath: 'en/docs',
            label: 'Docs',
            position: 'left' as const,
        },
        {
            to: 'en/docs/python-typing-5-minutes/',
            activeBasePath: 'en/docs/python-typing-5-minutes',
            label: 'Learn',
            position: 'left' as const,
        },
        {
            to: 'try/',
            activeBasePath: 'try',
            label: 'Sandbox',
            position: 'left' as const,
        },
        // TODO (T221099224) remove this check when we are ready to publish Installation doc to public
        process.env.INTERNAL_STATIC_DOCS === '1' ?
        {
            to: 'en/docs/fb/installation/',
            activeBasePath: 'en/docs/fb/installation',
            label: 'Install',
            position: 'left' as const,
        } : null,
        // Please keep GitHub link to the right for consistency.
        {
            href: 'https://github.com/facebook/pyrefly',
            'aria-label': 'GitHub',
            position: 'right' as const,
            className: 'navbar__icon github__link',
        },
    ].filter((x): x is NonNullable<typeof x> => x != null);
}

const config: Config = {
    title: 'Pyrefly',
    tagline: 'A Static Type Checker for Python',
    url: 'https://pyrefly.org',
    baseUrl: process.env.DOCUSAURUS_BASE_URL || '/',
    onBrokenLinks: 'throw',
    onBrokenMarkdownLinks: 'warn',
    favicon: 'img/Pyrefly-symbol.png',
    organizationName: 'facebook', // Usually your GitHub org/user name.
    projectName: 'Pyre', // Usually your repo name.
    trailingSlash: true,
    markdown: {
        mermaid: true,
    },
    themes: ['@docusaurus/theme-mermaid'],
    // We likely won't be able to use the faster docusaurus build for this website due to
    // the custom configuration in the plugins section. We would need another way to import these
    // plugins into the build if we want to use the experimental_faster option.
    // See https://fb.workplace.com/groups/docusaurus/posts/2314262428945865/?comment_id=2314271988944909
    // for more details.
    // future: {
    //   experimental_faster: true,
    // },
    plugins: [
        function polyfillNodeBuiltinsForFlowJS(context: any, options: any) {
            return {
                name: 'polyfillNodeBuiltinsForFlowJS',
                configureWebpack() {
                    return { resolve: { fallback: { fs: false, constants: false } } };
                },
            };
        },
        function enableSomeEnvVarsAsBuildTimeConstants() {
            return {
                name: 'enableSomeEnvVarsAsBuildTimeConstants',
                configureWebpack() {
                    return {
                        plugins: [
                            new webpack.EnvironmentPlugin({
                                INTERNAL_STATIC_DOCS:
                                    process.env.INTERNAL_STATIC_DOCS === '1' || false,
                            }),
                        ],
                    };
                },
            };
        },
        function enableMonacoEditorPlugin() {
            return {
                name: 'enableMonacoEditorPlugin',
                configureWebpack() {
                    return {
                        // https://stackoverflow.com/questions/69265357/monaco-editor-worker
                        plugins: [new MonacoWebpackPlugin()],
                    };
                },
            };
        },
        function enablePyodidePlugin() {
            return {
                name: 'enablePyodidePlugin',
                configureWebpack() {
                    return {
                        plugins: [new PyodidePlugin()],
                    };
                },
            };
        },
        function enableStyleX(context: any, options: any) {
            return {
                name: 'stylex-docusaurus',
                injectHtmlTags() {
                    return {
                        headTags: [
                            {
                                tagName: 'link',
                                attributes: {
                                    rel: 'stylesheet',
                                    href: context.baseUrl + 'stylex.css',
                                },
                            },
                        ],
                    };
                },

                configureWebpack(config: any, isServer: boolean, utils: any) {
                    const dev = config.mode === 'development';

                    return {
                        plugins: [
                            new StylexPlugin({
                                dev,
                                genConditionalClasses: true,
                                treeshakeCompensation: true,
                                unstable_moduleResolution: {
                                    type: 'commonJS',
                                    rootDir: context.siteDir,
                                },
                                filename: 'stylex.css',
                            }),
                        ],
                    };
                },
            };
        },
        [
            '@docusaurus/plugin-client-redirects',
            {
                redirects: [
                    {
                        from: '/en/docs/learn-python-typing',
                        to: '/en/docs/python-typing-for-beginners',
                    },
                ],
            },
        ],
    ],
    themeConfig: {
        prism: {
            theme: prismThemes.github,
        },
        colorMode: {
            defaultMode: 'light',
            disableSwitch: true,
            respectPrefersColorScheme: false,
        },
        navbar: {
            title: 'Pyrefly',
            items: getNavBarItems(),
        },
        footer: {
            style: 'dark',
            links: [
                {
                    title: 'Community',
                    items: [
                        {
                            label: 'Discord',
                            href: 'https://discord.gg/Cf7mFQtW7W',
                        },
                    ],
                },
                {
                    title: 'More',
                    items: [
                        {
                            label: 'Github',
                            href: 'https://github.com/facebook/pyrefly',
                        },
                    ],
                },
                {
                    title: 'Legal',
                    // Please do not remove the privacy and terms, it's a legal requirement.
                    items: [
                        {
                            label: 'Privacy',
                            href: 'https://opensource.facebook.com/legal/privacy/',
                        },
                        {
                            label: 'Terms',
                            href: 'https://opensource.facebook.com/legal/terms/',
                        },
                        {
                            label: 'Data Policy',
                            href: 'https://opensource.facebook.com/legal/data-policy/',
                        },
                        {
                            label: 'Cookie Policy',
                            href: 'https://opensource.facebook.com/legal/cookie-policy/',
                        },
                    ],
                },
            ],
            logo: {
                alt: 'Meta Open Source Logo',
                src: 'img/meta_open_source_logo.svg',
                href: 'https://opensource.fb.com/',
            },
            // Please do not remove the credits, help to publicize Docusaurus :)
            copyright: `Copyright © ${new Date().getFullYear()} Meta Platforms, Inc. Built with Docusaurus.`,
        },
    },
    customFields: {
        fbRepoName: 'fbsource',
    },
    presets: [
        [
            require.resolve('docusaurus-plugin-internaldocs-fb/docusaurus-preset'),
            {
                docs: {
                    routeBasePath: 'en/docs',
                    sidebarPath: require.resolve('./sidebars.ts'),
                    editUrl: fbContent({
                        internal:
                            'https://www.internalfb.com/code/fbsource/fbcode/pyrefly/website/',
                        external: 'https://github.com/facebook/pyrefly/edit/main/website/',
                    }),
                },
                staticDocsProject: 'Pyrefly',
                theme: {
                    customCss: require.resolve('./src/css/custom.css'),
                },
                enableEditor: true,
                gtag:
                    process.env.INTERNAL_STATIC_DOCS === '1'
                        ? undefined
                        : { trackingID: 'G-GSX14JC495', anonymizeIP: true },
            },
        ],
    ],
    scripts: [
        'https://buttons.github.io/buttons.js',
        'https://cdnjs.cloudflare.com/ajax/libs/clipboard.js/2.0.0/clipboard.min.js',
        '/js/code-block-buttons.js',
      ],
      stylesheets: ['/css/code-block-buttons.css']
};

export default config;
