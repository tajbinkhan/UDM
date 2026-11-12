//! Help > Tip of the Day: short tips about the Tauri window, in the order
//! they are shown. They name its own menus and pages.

pub const TIPS: [&str; 10] = [
    "Hold Alt while clicking a link to keep its download in the browser, or Ctrl to send any link to UDM. Options > General > Keys sets the keys.",
    "Right-click a link in the browser and choose Download later with UDM to add it to a queue without starting it.",
    "A queue can start on a schedule and put the computer to sleep or shut it down when it is done. Downloads > Scheduler sets one up.",
    "Tasks > Run site grabber finds the files of a type that a site links to, such as every PDF in its manuals.",
    "Tasks > Add batch download adds a numbered series of files, such as img001.jpg to img120.jpg, with one address.",
    "Ctrl+F finds a download in the list by its name, description or address, and F3 finds the next one.",
    "Options > Proxy / Socks sets a proxy, and Options > Sites Logins the logins for sites that ask for one.",
    "A category can remember the folder you last chose for it in Download File Info: tick its box under Options > Save to.",
    "Options > Downloads > Virus checking can scan each finished download for viruses.",
    "View > Customize URL List picks the list's columns and their order; dragging a column header's edge sets its width.",
];
