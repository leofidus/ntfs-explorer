use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    sync::mpsc,
};

use bytesize::ByteSize;
use chrono::{DateTime, Utc};
use itertools::{enumerate, Itertools};
use ntfs::{
    indexes::NtfsFileNameIndex,
    structured_values::{NtfsFileName, NtfsFileNamespace, NtfsObjectId, NtfsStandardInformation},
    Ntfs, NtfsAttributeType, NtfsFile,
};
use sector_reader::SectorReader;

mod sector_reader;

slint::include_modules!();

enum Command {
    EnterSubdir(String),
    MoveToParent(),
}

fn main() -> anyhow::Result<()> {
    let ui = MainWindow::new();

    let (tx, rx) = mpsc::channel();
    let ui_handle = ui.as_weak();
    std::thread::spawn(move || -> anyhow::Result<()> {
        //let f = File::open(r"\\.\C:")?;
        let f = File::open(
            r"C:\Users\JannisFroese\Downloads\dd-0.6beta3\links-quota-defaultBlocks-8GB.img",
        )?;
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
                Command::MoveToParent() => {
                    if current_directory.len() > 1 {
                        current_directory.pop();

                        show_dir(&current_directory, &mut fs, &ntfs, &ui_handle)?;
                    }
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

    let tx1 = tx.clone();
    ui.on_move_to_parent(move || {
        tx1.send(Command::MoveToParent()).unwrap();
    });

    // ui.on_show_details(move |x| {
    //     dbg!(x);
    // });

    // let file_model: Vec<FileItem> = ui.get_file_model().iter().collect();

    // let ui_handle = ui.as_weak();
    // ui.on_request_increase_value(move || {
    //     let ui = ui_handle.unwrap();
    //     ui.set_counter(ui.get_counter() + 1);
    // });

    ui.run();

    Ok(())
}

#[derive(Debug, Clone)]
struct StringFileProperty {
    name: String,
    value: String,
}

#[derive(Debug, Clone)]
struct StringFilePropertySection {
    headline: String,
    values: Vec<StringFileProperty>,
}

fn show_dir<T>(
    current_directory: &[ntfs::NtfsFile],
    fs: &mut T,
    ntfs: &Ntfs,
    ui: &slint::Weak<MainWindow>,
) -> Result<(), anyhow::Error>
where
    T: Read + Seek,
{
    let dir = current_directory.last().unwrap();
    let index = dir.directory_index(fs)?;
    let mut iter = index.entries();
    let mut file_model = vec![];
    let mut properties_model = vec![];

    let parent_record_number = dir.file_record_number();
    let mut files = vec![];
    while let Some(entry) = iter.next(fs) {
        let entry = entry?;
        let file = entry.to_file(ntfs, fs)?;
        let record_number = file.file_record_number();
        files.push((file, record_number));
    }
    let files = files
        .into_iter()
        .unique_by(|x| x.1)
        .map(|x| (best_file_name(fs, &x.0, parent_record_number), x.0))
        .map(|(a, b)| match a {
            Ok(a) => Ok((a, b)),
            Err(err) => Err(err),
        })
        .filter_map(|x| x.ok())
        .collect_vec();

    for (i, (filename, file)) in enumerate(&files) {
        // let filename = filename?;
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
            id: i as i32,
            attributes: attributes.into(),
            filename: filename_str.clone().into(),
            selected: false,
            size: file_size.clone().into(),
            is_directory,
        });
        properties_model.push(vec![StringFilePropertySection {
            headline: "General".into(),
            values: vec![
                StringFileProperty {
                    name: "Filename".into(),
                    value: filename_str,
                },
                StringFileProperty {
                    name: "Size".into(),
                    value: file_size,
                },
            ],
        }]);

        // let prefix = if file_name.is_directory() {
        //     "<DIR>"
        // } else {
        //     ""
        // };
        // println!("{:5}  {}", prefix, file_name.name());
    }
    let properties2 = properties_model.clone();
    ui.upgrade_in_event_loop(|ui| {
        let file_model = std::rc::Rc::new(slint::VecModel::from(file_model));
        let properties_model = std::rc::Rc::new(slint::VecModel::from(
            properties2
                .into_iter()
                .map(|f| {
                    std::rc::Rc::new(slint::VecModel::from(
                        f.into_iter()
                            .map(|v| FilePropertySection {
                                headline: v.headline.into(),
                                values: std::rc::Rc::new(slint::VecModel::from(
                                    v.values
                                        .into_iter()
                                        .map(|u| FileProperty {
                                            name: u.name.into(),
                                            value: u.value.into(),
                                        })
                                        .collect_vec(),
                                ))
                                .into(),
                            })
                            .collect_vec(),
                    ))
                    .into()
                })
                .collect_vec(),
        ));
        // let properties = vec![FilePropertySection {
        //     headline: "General".into(),
        //     values: std::rc::Rc::new(slint::VecModel::from(vec![
        //         FileProperty {
        //             name: "Size".into(),
        //             value: "412 kB".into(),
        //         },
        //         FileProperty {
        //             name: "Filename".into(),
        //             value: "example.txt".into(),
        //         },
        //     ]))
        //     .into(),
        // }];

        ui.set_file_model(file_model.into());
        ui.set_file_properties(properties_model.into());
        //ui.set_file_property_sections(std::rc::Rc::new(slint::VecModel::from(properties)).into());
        ui.set_scroll_y(0.0);
    });

    for (i, (_filename, file)) in enumerate(files) {
        let properties = get_file_attributes(fs, ntfs, &file, parent_record_number)?;
        let mut string_properties = vec![];
        for standard_information in properties.standard_informations {
            string_properties.push(StringFilePropertySection {
                headline: "General".into(),
                values: vec![
                    StringFileProperty {
                        name: "Creation".into(),
                        value: DateTime::from(standard_information.creation_time()).to_string(),
                    },
                    StringFileProperty {
                    name: "Last Access".into(),
                    value: DateTime::from(standard_information.access_time()).to_string(),
                    },
                    StringFileProperty {
                        name: "Modification".into(),
                        value: DateTime::from(standard_information.modification_time()).to_string(),
                    },
                    StringFileProperty {
                        name: "MFT Record Modification".into(),
                        value: DateTime::from(standard_information.mft_record_modification_time())
                            .to_string(),
                    },
                ],
            })
        }
        properties_model[i] = string_properties;
    }

    ui.upgrade_in_event_loop(|ui| {
        let properties_model = std::rc::Rc::new(slint::VecModel::from(
            properties_model
                .into_iter()
                .map(|f| {
                    std::rc::Rc::new(slint::VecModel::from(
                        f.into_iter()
                            .map(|v| FilePropertySection {
                                headline: v.headline.into(),
                                values: std::rc::Rc::new(slint::VecModel::from(
                                    v.values
                                        .into_iter()
                                        .map(|u| FileProperty {
                                            name: u.name.into(),
                                            value: u.value.into(),
                                        })
                                        .collect_vec(),
                                ))
                                .into(),
                            })
                            .collect_vec(),
                    ))
                    .into()
                })
                .collect_vec(),
        ));
        ui.set_file_properties(properties_model.into());
    });

    Ok(())
}

#[derive(Debug)]
struct FileAttributes {
    filenames: Vec<(NtfsFileNamespace, String, NtfsFileName)>,
    hard_links: Vec<(NtfsFileNamespace, String, NtfsFileName)>,
    standard_informations: Vec<NtfsStandardInformation>,
    object_ids: Vec<NtfsObjectId>,
}

fn get_file_attributes<T>(
    fs: &mut T,
    ntfs: &Ntfs,
    file: &NtfsFile,
    parent_dir: u64,
) -> anyhow::Result<FileAttributes>
where
    T: Read + Seek,
{
    let mut standard_informations = vec![];
    let mut filenames = vec![];
    let mut hard_links = vec![];
    let mut object_ids = vec![];
    let own_record_number = file.file_record_number();
    let mut attributes = file.attributes();
    while let Some(attr) = attributes.next(fs) {
        if let Ok(attr) = attr {
            let attr = attr.to_attribute();
            // dbg!(attr.ty()?);
            // dbg!(attr.position());
            dbg!(best_file_name(fs, file, parent_dir)?
                .name()
                .to_string_lossy());
            if attr.ty().is_err() {
                eprint!("unknown attribute type: {:?}", attr.name());
            }
            match attr.ty().unwrap() {
                NtfsAttributeType::StandardInformation => {
                    let data: NtfsStandardInformation = attr.structured_value(fs).unwrap();
                    standard_informations.push(data);
                }
                NtfsAttributeType::AttributeList => continue,
                NtfsAttributeType::FileName => {
                    let data: NtfsFileName = attr.structured_value(fs).unwrap();
                    if data.parent_directory_reference().file_record_number() == parent_dir {
                        filenames.push((data.namespace(), data.name().to_string_lossy(), data));
                    } else {
                        let ns = data.namespace();
                        let mut path = vec![data.name().to_string_lossy()];
                        let mut parent = data.parent_directory_reference();
                        let mut current_file_record_number = own_record_number;
                        while parent.file_record_number() != current_file_record_number {
                            let parent_dir = parent.to_file(ntfs, fs).unwrap();
                            let ntfs_file_name: Option<NtfsFileName> =
                                match parent_dir.name(fs, Some(ns), None) {
                                    Some(name) => Some(name.unwrap()),
                                    None => {
                                        parent_dir.name(fs, None, None).map(|name| name.unwrap())
                                    }
                                };
                            dbg!(&ntfs_file_name);
                            match ntfs_file_name {
                                Some(name) => {
                                    current_file_record_number = parent_dir.file_record_number();
                                    path.push(name.name().to_string_lossy());
                                    parent = name.parent_directory_reference();
                                }
                                None => {
                                    path.push("[[no file name found]]".to_owned());
                                    break;
                                }
                            };
                        }
                        path.reverse();
                        let path = path.join(r"\");
                        hard_links.push((data.namespace(), path, data));
                    }
                }
                NtfsAttributeType::ObjectId => {
                    let data: NtfsObjectId = attr.structured_value(fs).unwrap();
                    object_ids.push(data);
                }
                _ => continue,
                NtfsAttributeType::SecurityDescriptor => todo!(),
                NtfsAttributeType::VolumeName => todo!(),
                NtfsAttributeType::VolumeInformation => todo!(),
                NtfsAttributeType::Data => todo!(),
                NtfsAttributeType::IndexRoot => todo!(),
                NtfsAttributeType::IndexAllocation => todo!(),
                NtfsAttributeType::Bitmap => todo!(),
                NtfsAttributeType::ReparsePoint => todo!(),
                NtfsAttributeType::EAInformation => todo!(),
                NtfsAttributeType::EA => todo!(),
                NtfsAttributeType::PropertySet => todo!(),
                NtfsAttributeType::LoggedUtilityStream => todo!(),
                NtfsAttributeType::End => todo!(),
            }
        }
    }

    Ok(FileAttributes {
        filenames,
        hard_links,
        standard_informations,
        object_ids,
    })
}

fn best_file_name<T>(
    fs: &mut T,
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
