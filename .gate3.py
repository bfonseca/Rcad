A = "crates/rcad-app/src/app.rs"
src = open(A).read()

old = "        fn new(app: RcadApp) -> Self {"
new = ('        /// A session with a throwaway project open: the modelling UI is',
       '        /// unreachable without one.',
       '        fn new(app: RcadApp) -> Self {',
       '            let mut app = app;',
       '            if app.project_root.is_none() {',
       '                app.open_project_folder(temp_project_dir());',
       '            }',
       '            Self::without_project(app)',
       '        }',
       '',
       '        /// A session with no project at all, to test the gate itself.',
       '        fn without_project(app: RcadApp) -> Self {',
       '            let _ = &app;')
print("Session::new encontrado:", src.count(old))
src = src.replace(old, new, 1)

anchor = "    use super::*;"
if "fn temp_project_dir" not in src:
    src = src.replace(anchor, anchor + "\n\n" + open(".tmph3.txt").read(), 1)
    src = src.rstrip("\n") + "\n\n" + open(".tests3.txt").read()
    print("testes do gate adicionados")
open(A, "w").write(src)