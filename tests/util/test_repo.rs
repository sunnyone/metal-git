extern crate git2;
extern crate tempdir;
extern crate metal_git;

use git2::{Repository, Signature, Commit, BranchType};
use std::thread::sleep;
use tempdir::TempDir;
use std::time::Duration;
use std::path::Path;
use std::cell::Cell;
use std::rc::Rc;
use std::process::Command;

use metal_git::repository_manager::RepositoryManager;

pub struct TestRepo {
	temp_dir: TempDir,
	debug_mode: Cell<bool>
}

impl Drop for TestRepo {
	fn drop(&mut self) {
		if self.debug_mode.get() {
			println!("TestRepo is here: {}", self.temp_dir.path().to_string_lossy());
			
			let output = Command::new("sh")
			                    .arg("-c")
			                    .arg("git log --graph --pretty=format:%s --date-order | sed -e 's/^/    \\/\\/ /g'")
			                    // .arg("git log --graph --oneline --date-order | sed -e 's/^/    \\/\\/ /g'")
			                    .current_dir(self.temp_dir.path())
			                    .output()
			                    .expect("Failed to execute sh/git command");
			                    
			println!("git log: \n{}", String::from_utf8_lossy(&output.stdout));
			
			println!("Waiting...");
			sleep(Duration::from_secs(60));
			println!("Done");
		}
	}
}

fn test_commit<'repo, 'a>(repo: &'repo Repository,
	branch_name: &'repo str,
	message: &'repo str, 
	parents: &'a [&'a Commit]) -> Commit<'repo> {
	let signature = Signature::now("test commit", "test@example.com").unwrap();
	
	let treebuilder = repo.treebuilder(None).unwrap();
	let tree_oid = treebuilder.write().unwrap();
	let tree = repo.find_tree(tree_oid).unwrap();
	
	// TODO: correct check
	let branch_exists = repo.find_branch(branch_name, BranchType::Local).is_ok();
	
	let ref_name = format!("refs/heads/{}", branch_name);
	let commit_oid = repo.commit(if branch_exists { Some(&ref_name) } else { None },
		&signature,
		&signature,
		message,
		&tree,
		parents
	).expect("Failed to commit");

	let commit = repo.find_commit(commit_oid).unwrap();
	
	if !branch_exists {
		repo.branch(branch_name, &commit, true).unwrap();
	}
	
	commit
}

#[allow(dead_code)]
impl TestRepo {
	fn new(prefix: &str) -> TestRepo {
		let prefix_testrepo = format!("testrepo-{}", prefix);
		let temp_dir = TempDir::new(&prefix_testrepo).expect("Failed to create tempdir");
		let _ = Repository::init(temp_dir.path()).unwrap();
		
		TestRepo {
			temp_dir: temp_dir,
			debug_mode: Cell::new(false)
		}
	}
	
	pub fn path(&self) -> &Path {
		self.temp_dir.path()
	}
	
	pub fn repository_manager(&self) -> Rc<RepositoryManager> {
		let r = RepositoryManager::new();
	    r.set_work_dir_path(self.path().to_str().unwrap());
	    r 
	}
	
	fn repository(&self) -> Repository {
		Repository::open(self.path()).expect("Failed to open a test repository.")
	}
	
	#[allow(dead_code)]
	pub fn set_debug(&self) {
		self.debug_mode.set(true);
	}
	
	pub fn empty() -> TestRepo {
		let test_repo = Self::new("empty");
		test_repo
	}
	
	// get comments with set_debug()
    // * Single commit
	pub fn single() -> TestRepo {
		let test_repo = Self::new("single");
		let repo = test_repo.repository();
	    test_commit(&repo, "master", "Single commit", &[]);
	    
		test_repo
	}
	
    // * B
    // * A
	pub fn flat_two() -> TestRepo {
		let test_repo = Self::new("flat-two");
		let repo = test_repo.repository();
		
	    let a = test_commit(&repo, "master", "A", &[]);
	    let _ = test_commit(&repo, "master", "B", &[&a]);
	    
		test_repo
	}
	
    // *   D
    // |\  
    // * | C
    // | * B
    // |/  
    // * A
	pub fn two_parent_two_child() -> TestRepo {
		let test_repo = Self::new("two_parent_two_child");
		let repo = test_repo.repository();
		
		let a = test_commit(&repo, "branch1", "A", &[]);
		let b = test_commit(&repo, "branch1", "B", &[&a]);
		let c = test_commit(&repo, "master", "C", &[&a]);
		
		let _ = test_commit(&repo, "master", "D", &[&c,&b]);
		
		test_repo
	}
	

	pub fn branch_merge_branch_merge() -> TestRepo {
		let test_repo = Self::new("branch_merge_branch_merge");
		
		let repo = test_repo.repository();
		
		// FIXME: how to create time ordered fast?
		let a = test_commit(&repo, "master", "A", &[]);
		sleep(Duration::from_secs(1));
		let b = test_commit(&repo, "branch1", "B", &[&a]);
		sleep(Duration::from_secs(1));
		let c = test_commit(&repo, "branchX", "C", &[&a]); // most outer line
		sleep(Duration::from_secs(1));
		let d = test_commit(&repo, "master", "D", &[&a,&b]);
		sleep(Duration::from_secs(1));
		let e = test_commit(&repo, "branch2", "E", &[&d]);
		sleep(Duration::from_secs(1));
		let _ = test_commit(&repo, "master", "F", &[&d,&c,&e]);
		
		test_repo
	}
}