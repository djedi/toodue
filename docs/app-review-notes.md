# App Review notes (App Store Connect → App Review Information → Notes)

Paste the section below into the Notes field and into the reply to App Review. Fill in the
`<…>` placeholders first. Both demo accounts must exist on app.toodue.com and share the
"Demo — Team Project" project so the reviewer can try sharing, reporting and blocking.

---

**1. Screen recording**
Attached separately. It is recorded on a physical iPhone running the latest iOS and starts at app launch.
It shows these flows:
- registration and login
- creating tasks and projects
- sharing a project and commenting
- reporting and blocking
- account deletion

**2. Purpose and target audience**
TooDue is a to-do list and task manager for individuals, families and small teams.

People use it to capture tasks quickly and organize them into Inbox, Today, Upcoming and Projects. Tasks
can have due dates, repeating schedules, deadlines, priorities, sub-tasks, comments and attachments.

The problem it solves is keeping personal and shared to-dos in one fast, simple place that stays in
sync across iPhone, Android and the web. Projects can be shared with other TooDue users, so a household
or a small team can work from the same list in real time.

The app is free. It has no ads, no in-app purchases and no paid content.

**3. Setup and access**

Demo account A: `<demo-a email>` / `<password>`
Demo account B: `<demo-b email>` / `<password>`

1. Launch the app and log in with demo account A. (To test registration instead, tap "Sign up", enter
   a name, email and password, and agree to the Terms.)
2. **Tasks:** tap + to add a task. Tap a task to open it and edit its date, priority, sub-tasks,
   comments or attachments. Tap the circle to complete it.
3. **Projects:** open the Projects tab. "Demo — Team Project" is shared with demo account B.
4. **Sharing:** open a project and tap its settings. Under Members you can invite another TooDue user
   by email.
5. **Reporting user-generated content:** in "Demo — Team Project", open a task that demo account B
   created.
   - Tap the shield icon in the task header to report the task.
   - Tap "Report" on any of B's comments to report the comment.
   - Tap the shield icon next to an attachment to report the attachment.
   - In project settings → Members, tap "Report" next to B to report the person.

   Every report asks for a reason and has an "Also block" option. We review reports within 24 hours.
   We remove objectionable content and ban offending accounts.
6. **Blocking:** tap "Block" next to a comment, or next to a member in project settings. Blocking does
   four things:
   - removes the blocked person from your projects
   - removes you from their projects
   - stops them from sharing projects with you
   - hides their comments from you

   Blocked people are listed under Settings → Blocked people, where you can unblock them.
7. **Terms:** users must agree to the Terms of Service, which forbid objectionable content and abusive
   behavior, before creating an account. Terms, Privacy and Support links are on the sign-in screen
   and in Settings.
8. **Account deletion:** open Settings (gear icon) → Delete account, then enter the password to
   confirm. The account and its data are permanently deleted immediately.

**4. External services**
- **TooDue's own server (app.toodue.com):** accounts, data storage and real-time sync. We operate it
  ourselves on rented hosting. There is no third-party backend, analytics, advertising or tracking SDK.
- **Google Calendar API (optional):** used only if the user chooses to connect Google Calendar in
  Settings, to sync tasks with due dates to their calendar. It uses Google OAuth.
- **AI assistants via MCP (optional):** users can connect their own AI agent (for example Claude or
  ChatGPT) to their TooDue account through OAuth or an API key, so the agent can manage their tasks.
  TooDue itself does not call any AI service.
- **No payment processors and no third-party authentication:** sign-in uses email and password.

**5. Regional differences**
None. The app works the same way in every region and is available in English.

**6. Regulated industries / third-party material**
Not applicable. TooDue is not in a regulated industry and includes no protected third-party content.
