use std::{fs::File, io::BufReader};

use bytesize::ByteSize;
use ntfs::{structured_values::NtfsFileNamespace, Ntfs};
use sector_reader::SectorReader;
use slint::Model;

mod sector_reader;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let ui = MainWindow::new();

    let f = File::open(r"\\.\C:")?;
    let sr = SectorReader::new(f, 512)?;
    let mut fs = BufReader::new(sr);
    let mut ntfs = Ntfs::new(&mut fs)?;
    ntfs.read_upcase_table(&mut fs)?;
    let current_directory = vec![ntfs.root_directory(&mut fs)?];

    let index = current_directory.last().unwrap().directory_index(&mut fs)?;
    let mut iter = index.entries();

    let mut file_model = vec![];

    while let Some(entry) = iter.next(&mut fs) {
        let entry = entry?;
        let file = entry.to_file(&ntfs, &mut fs)?;
        let attributes = format!("{:?}", file.info()?.file_attributes());
        let file_size = format!(
            "{}",
            ByteSize(
                file.data(&mut fs, "")
                    .transpose()?
                    .map(|d| d.to_attribute().value_length())
                    .unwrap_or_default(),
            ),
        );

        let file_name = entry
            .key()
            .expect("key must exist for a found Index Entry")?;
        let is_directory = file_name.is_directory();
        println!(
            "{}, keyspace: {:?}",
            file_name.name().to_string_lossy(),
            file_name.namespace()
        );
        if file_name.namespace() == NtfsFileNamespace::Dos {
            continue;
        }
        let file_name = file_name.name().to_string_lossy();

        file_model.push(FileItem {
            attributes: attributes.into(),
            filename: file_name.into(),
            selected: false,
            size: file_size.into(),
            is_directory,
        });

        // let prefix = if file_name.is_directory() {
        //     "<DIR>"
        // } else {
        //     ""
        // };
        // println!("{:5}  {}", prefix, file_name.name());
    }

    let file_model = std::rc::Rc::new(slint::VecModel::from(file_model));

    ui.set_file_model(file_model.into());

    // let file_model: Vec<FileItem> = ui.get_file_model().iter().collect();

    // let ui_handle = ui.as_weak();
    // ui.on_request_increase_value(move || {
    //     let ui = ui_handle.unwrap();
    //     ui.set_counter(ui.get_counter() + 1);
    // });

    ui.run();

    Ok(())
}
