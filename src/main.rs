use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    sync::mpsc,
};

use bytesize::ByteSize;
use itertools::Itertools;
use ntfs::{
    indexes::NtfsFileNameIndex,
    structured_values::{NtfsFileName, NtfsFileNamespace},
    Ntfs, NtfsFile,
};
use sector_reader::SectorReader;

mod sector_reader;

slint::include_modules!();

enum Command {
    EnterSubdir(String),
}

fn main() -> anyhow::Result<()> {
    let ui = MainWindow::new();

    let (tx, rx) = mpsc::channel();
    let ui_handle = ui.as_weak();
    std::thread::spawn(move || -> anyhow::Result<()> {
        //let f = File::open(r"\\.\C:")?;
        let f = File::open(r"E:\Backups\c_ssd2021_raw.img")?;
        let sr = SectorReader::new(f, 512)?;
        let mut fs = BufReader::new(sr);
        let mut ntfs = Ntfs::new(&mut fs)?;
        ntfs.read_upcase_table(&mut fs)?;
        let mut current_directory = vec![ntfs.root_directory(&mut fs)?];

        show_dir(&current_directory, &mut fs, &ntfs, &ui_handle)?;

        loop {
            match rx.recv().unwrap() {
                Command::EnterSubdir(dir_name) => {
                    let index = current_directory
                        .last()
                        .unwrap()
                        .directory_index(&mut fs)
                        .unwrap();
                    let mut finder = index.finder();
                    let maybe_entry =
                        NtfsFileNameIndex::find(&mut finder, &ntfs, &mut fs, dir_name.as_str());

                    if maybe_entry.is_none() {
                        continue;
                    }
                    let entry = maybe_entry.unwrap();
                    let file = entry.unwrap().to_file(&ntfs, &mut fs).unwrap();
                    current_directory.push(file);

                    show_dir(&current_directory, &mut fs, &ntfs, &ui_handle)?;
                }
            }
        }

        // Ok(())
    });

    let tx1 = tx.clone();
    ui.on_enter_directory(move |dir_name| {
        println!("enter dir {}", dir_name);
        tx1.send(Command::EnterSubdir(dir_name.to_string()))
            .unwrap();
    });

    // let file_model: Vec<FileItem> = ui.get_file_model().iter().collect();

    // let ui_handle = ui.as_weak();
    // ui.on_request_increase_value(move || {
    //     let ui = ui_handle.unwrap();
    //     ui.set_counter(ui.get_counter() + 1);
    // });

    ui.run();

    Ok(())
}

fn show_dir(
    current_directory: &[ntfs::NtfsFile],
    fs: &mut BufReader<SectorReader<File>>,
    ntfs: &Ntfs,
    ui: &slint::Weak<MainWindow>,
) -> Result<(), anyhow::Error> {
    let dir = current_directory.last().unwrap();
    let index = dir.directory_index(fs)?;
    let mut iter = index.entries();
    let mut file_model = vec![];

    let parent_record_number = dir.file_record_number();
    let mut files = vec![];
    while let Some(entry) = iter.next(fs) {
        let entry = entry?;
        let file = entry.to_file(&ntfs, fs)?;
        let record_number = file.file_record_number();
        files.push((file, record_number));
    }
    let files = files
        .into_iter()
        .unique_by(|x| x.1)
        .map(|x| (best_file_name(fs, &x.0, parent_record_number), x.0))
        .collect_vec();

    for (filename, file) in files {
        let filename = filename?;
        let attributes = format!("{:?}", file.info()?.file_attributes());
        let file_size = format!(
            "{}",
            ByteSize(
                file.data(fs, "")
                    .transpose()?
                    .map(|d| d.to_attribute().value_length())
                    .unwrap_or_default(),
            ),
        );
        // println!("file {}", file.file_record_number());

        // let file_name = entry
        //     .key()
        //     .expect("key must exist for a found Index Entry")?;
        // let is_directory = file_name.is_directory();
        // println!(
        //     "{}, keyspace: {:?}",
        //     file_name.name().to_string_lossy(),
        //     file_name.namespace()
        // );
        // if file_name.namespace() == NtfsFileNamespace::Dos {
        //     continue;
        // }
        let is_directory = filename.is_directory();
        let filename_str = filename.name().to_string_lossy();

        file_model.push(FileItem {
            attributes: attributes.into(),
            filename: filename_str.into(),
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
    ui.upgrade_in_event_loop(|ui| {
        let file_model = std::rc::Rc::new(slint::VecModel::from(file_model));
        let properties = vec![FilePropertySection {
            headline: "General".into(),
            values: std::rc::Rc::new(slint::VecModel::from(vec![
                FileProperty {
                    name: "Size".into(),
                    value: "412 kB".into(),
                },
                FileProperty {
                    name: "Filename".into(),
                    value: "example.txt".into(),
                },
            ]))
            .into(),
        }];

        ui.set_file_model(file_model.into());
        ui.set_file_property_sections(std::rc::Rc::new(slint::VecModel::from(properties)).into());
    });
    Ok(())
}

fn best_file_name<T>(
    fs: &mut BufReader<T>,
    file: &NtfsFile,
    parent_record_number: u64,
) -> anyhow::Result<NtfsFileName>
where
    T: Read + Seek,
{
    // Try to find a long filename (Win32) first.
    // If we don't find one, the file may only have a single short name (Win32AndDos).
    // If we don't find one either, go with any namespace. It may still be a Dos or Posix name then.
    let priority = [
        Some(NtfsFileNamespace::Win32),
        Some(NtfsFileNamespace::Win32AndDos),
        None,
    ];

    for match_namespace in priority {
        if let Some(file_name) = file.name(fs, match_namespace, Some(parent_record_number)) {
            let file_name = file_name?;
            return Ok(file_name);
        }
    }

    panic!(
        "Found no FileName attribute for File Record {:#x}",
        file.file_record_number()
    )
}
