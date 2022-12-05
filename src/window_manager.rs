use std::rc::Rc;
use std::cell::RefCell;
use crate::history_window::HistoryWindow;
use crate::commit_window::CommitWindow;
use crate::repository_manager::RepositoryManager;

pub struct WindowManager {
    windows: RefCell<Option<Windows>>,
}

struct Windows {
    history_window: Rc<HistoryWindow>,
    commit_window: Rc<CommitWindow>,
}

impl WindowManager {
    pub fn new(repository_manager: Rc<RepositoryManager>) -> Rc<WindowManager> {
        let window_manager = Rc::new(WindowManager { windows: RefCell::new(None) });

        let windows = Windows {
            history_window: HistoryWindow::new(Rc::downgrade(&window_manager),
                                               repository_manager.clone()),
            commit_window: CommitWindow::new(repository_manager.clone()),
        };

        *window_manager.windows.borrow_mut() = Some(windows);

        window_manager
    }

    fn with_windows<F>(&self, func: F)
        where F: Fn(&Windows)
    {
        let windows_ref = self.windows.borrow();
        let windows = windows_ref.as_ref().unwrap();
        func(windows);
    }

    pub fn start(&self) {
        self.with_windows(|windows| {
            windows.history_window.connect_closed(|| {
                gtk::main_quit();
            });

            windows.history_window.show();
        });
    }

    pub fn show_commit_window(&self) {
        self.with_windows(|windows| {
            // TODO: messaging should be done in another class
            let w = Rc::downgrade(&windows.history_window);
            windows.commit_window.connect_commited(move || {
                w.upgrade().unwrap().refresh();
            });

            windows.commit_window.show();
        });
    }
}
