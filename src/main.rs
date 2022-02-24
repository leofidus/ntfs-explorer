slint::include_modules!();

fn main() {
    let ui = MainWindow::new();

    // normally dynamically generated, just fixed values for reproduction case
    let file_model = vec![
        FileItem {
            filename: "one".into(),
        },
        FileItem {
            filename: "two".into(),
        },
    ];

    let file_model = std::rc::Rc::new(slint::VecModel::from(file_model));
    ui.set_file_model(file_model.into());

    ui.run();
}
