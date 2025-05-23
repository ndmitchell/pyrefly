/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;

use anyhow::Context as _;
use anyhow::anyhow;
use starlark_map::small_map::SmallMap;
use tar::Archive;
use zstd::stream::read::Decoder;

use crate::module::module_name::ModuleName;
use crate::module::module_path::ModulePath;

const BUNDLED_TYPESHED_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/typeshed.tar.zst"));

#[derive(Debug, Clone)]
pub struct BundledTypeshed {
    find: SmallMap<ModuleName, PathBuf>,
    load: SmallMap<PathBuf, Arc<String>>,
}

impl BundledTypeshed {
    fn unpack() -> anyhow::Result<SmallMap<PathBuf, String>> {
        let decoder = Decoder::new(BUNDLED_TYPESHED_BYTES)?;
        let mut archive = Archive::new(decoder);
        let entries = archive
            .entries()
            .context("Cannot query all entries in typeshed archive")?;

        let mut items = SmallMap::new();
        for maybe_entry in entries {
            let mut entry =
                maybe_entry.context("Cannot read individual entry in typeshed archive")?;
            if entry.header().entry_type().is_dir() {
                // Skip directories
                continue;
            }
            let relative_path_context = entry
                .path()
                .context("Cannot extract path from archive entry")?;
            let mut relative_path_components = relative_path_context.components();
            let first_component = relative_path_components.next();
            if first_component.is_none_or(|component| component.as_os_str() != "stdlib") {
                // We bundle only the stdlib/ portion of typeshed.
                continue;
            }
            let relative_path = relative_path_components.collect::<PathBuf>();
            if relative_path.extension().is_none_or(|ext| ext != "pyi") {
                // typeshed/stdlib/ contains non-.pyi files like VERSIONS that we don't care about.
                continue;
            }
            let size = entry.size();
            let mut contents = String::with_capacity(size as usize);
            entry
                .read_to_string(&mut contents)
                .context("Cannot read content of archive entry")?;
            items.entry(relative_path).or_insert(contents);
        }
        Ok(items)
    }

    fn new() -> anyhow::Result<Self> {
        let contents = Self::unpack()?;
        let mut res = Self {
            find: SmallMap::new(),
            load: SmallMap::new(),
        };
        for (relative_path, contents) in contents {
            let module_name = ModuleName::from_relative_path(&relative_path)?;
            res.find.insert(module_name, relative_path.clone());
            res.load.insert(relative_path, Arc::new(contents));
        }
        Ok(res)
    }

    pub fn find(&self, module: ModuleName) -> Option<ModulePath> {
        self.find
            .get(&module)
            .map(|path| ModulePath::bundled_typeshed(path.clone()))
    }

    pub fn load(&self, path: &PathBuf) -> Option<Arc<String>> {
        self.load.get(path).cloned()
    }
}

static BUNDLED_TYPESHED: LazyLock<anyhow::Result<BundledTypeshed>> =
    LazyLock::new(BundledTypeshed::new);

pub fn typeshed() -> anyhow::Result<&'static BundledTypeshed> {
    match &*BUNDLED_TYPESHED {
        Ok(typeshed) => Ok(typeshed),
        Err(error) => Err(anyhow!("{error:#}")),
    }
}
