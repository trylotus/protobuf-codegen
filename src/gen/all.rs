use std::collections::HashMap;
use std::collections::HashSet;

use protobuf::descriptor::FileDescriptorProto;
use protobuf::reflect::FileDescriptor;
use protobuf_parse::ProtoPath;
use protobuf_parse::ProtoPathBuf;

use crate::compiler_plugin;
use crate::customize::ctx::CustomizeElemCtx;
use crate::customize::CustomizeCallback;
use crate::gen::file::gen_file;
use crate::gen::mod_rs::gen_mod_rs;
use crate::gen::scope::RootScope;
use crate::gen::well_known_types::gen_well_known_types_mod;
use crate::Customize;

pub(crate) fn gen_all(
    file_descriptors: &[FileDescriptorProto],
    parser: &str,
    files_to_generate: &[ProtoPathBuf],
    customize: &Customize,
    customize_callback: &dyn CustomizeCallback,
) -> anyhow::Result<Vec<compiler_plugin::GenResult>> {
    let mut dirs = HashMap::<String, Vec<ProtoPathBuf>>::new();

    for file in files_to_generate {
        let path = file.as_path().to_string();
        let last_slash = path.rfind("/");
        let dir = last_slash
            .map(|last_slash| &path[..last_slash])
            .unwrap_or("");

        if !dirs.contains_key(dir) {
            dirs.insert(dir.to_string(), Vec::new());
        }

        dirs.get_mut(dir).unwrap().push(file.clone());
    }

    let mut results: Vec<compiler_plugin::GenResult> = Vec::new();

    for (dir, files) in dirs {
        let mut same_dir_results = gen_all_same_dir(
            &dir,
            file_descriptors,
            parser,
            &files,
            customize,
            customize_callback,
        )?;

        results.append(&mut same_dir_results);
    }

    Ok(results)
}

pub(crate) fn gen_all_same_dir(
    dir: &str,
    file_descriptors: &[FileDescriptorProto],
    parser: &str,
    files_to_generate: &[ProtoPathBuf],
    customize: &Customize,
    customize_callback: &dyn CustomizeCallback,
) -> anyhow::Result<Vec<compiler_plugin::GenResult>> {
    let file_descriptors = FileDescriptor::new_dynamic_fds(file_descriptors.to_vec(), &[])?;

    let root_scope = RootScope {
        file_descriptors: &file_descriptors,
    };

    let mut results: Vec<compiler_plugin::GenResult> = Vec::new();
    let files_map: HashMap<&ProtoPath, &FileDescriptor> = file_descriptors
        .iter()
        .map(|f| Ok((ProtoPath::new(f.proto().name())?, f)))
        .collect::<Result<_, anyhow::Error>>()?;

    let mut mods = Vec::new();

    let customize = CustomizeElemCtx {
        for_elem: customize.clone(),
        for_children: customize.clone(),
        callback: customize_callback,
    };

    let mut files = HashSet::<&FileDescriptor>::new();

    for file_name in files_to_generate {
        let file = files_map.get(file_name.as_path()).expect(&format!(
            "file not found in file descriptors: {:?}, files: {:?}",
            file_name,
            files_map.keys()
        ));

        files.insert(file);

        for dep in file.deps() {
            if !dep.name().starts_with("google/protobuf") {
                files.insert(dep);
            }
        }
    }

    for file in files {
        let mut gen_file_result = gen_file(file, &files_map, &root_scope, &customize, parser)?;

        if !dir.is_empty() {
            gen_file_result.compiler_plugin_result.name =
                format!("{}/{}", dir, gen_file_result.compiler_plugin_result.name);
        }

        results.push(gen_file_result.compiler_plugin_result);
        mods.push(gen_file_result.mod_name);
    }

    if customize.for_elem.inside_protobuf.unwrap_or(false) {
        let mut result = gen_well_known_types_mod();

        if !dir.is_empty() {
            result.name = format!("{}/{}", dir, result.name);
        }

        results.push(result);
    }

    if customize.for_elem.gen_mod_rs.unwrap_or(true) {
        let mut result = gen_mod_rs(&mods);

        if !dir.is_empty() {
            result.name = format!("{}/{}", dir, result.name);
        }

        results.push(result);
    }

    Ok(results)
}
