using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;

namespace StudioSetup
{
    static class Program
    {
        [STAThread]
        static void Main(string[] args)
        {
            ServicePointManager.SecurityProtocol = (SecurityProtocolType)3072 | (SecurityProtocolType)768 | SecurityProtocolType.Tls;
            ServicePointManager.DefaultConnectionLimit = 32;

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new MainForm(args));
        }
    }

    public class ModelItem
    {
        public string Title { get; set; }
        public string Category { get; set; }
        public string Description { get; set; }
        public long SizeBytes { get; set; }
        public List<FileSpec> Files { get; set; }
        public bool IsSelected { get; set; }
        public string RegistryKey { get; set; }
        public string RegistryFamily { get; set; }

        public ModelItem()
        {
            Files = new List<FileSpec>();
            IsSelected = true;
        }

        public string DisplaySize
        {
            get
            {
                if (SizeBytes >= 1024L * 1024 * 1024)
                    return string.Format("{0:0.1} ГБ", (double)SizeBytes / (1024.0 * 1024 * 1024));
                return string.Format("{0:0.1} МБ", (double)SizeBytes / (1024.0 * 1024));
            }
        }
    }

    public class FileSpec
    {
        public string RelativePath { get; set; }
        public string Url { get; set; }
        public long ExpectedSize { get; set; }
        public string Description { get; set; }
    }

    public class MainForm : Form
    {
        private TextBox txtTarget;
        private Button btnBrowse;
        private Label lblDetect;
        private CheckedListBox clbModels;
        private CheckBox chkShortcut;
        private ProgressBar pbTotal;
        private ProgressBar pbFile;
        private Label lblStatus;
        private Label lblSpeed;
        private Button btnInstall;
        private Button btnCancel;
        private Button btnLaunch;
        private RichTextBox rtbLog;

        private BackgroundWorker worker;
        private volatile bool cancelRequested = false;
        private string appRoot;

        private List<ModelItem> catalog;

        public MainForm(string[] args)
        {
            InitCatalog();
            InitUI();
            DetectAppDirectory();
        }

        private void InitCatalog()
        {
            catalog = new List<ModelItem>();

            // 1. Translator (Helsinki-NLP/opus-mt-ru-en)
            var tr = new ModelItem
            {
                Title = "Офлайн-переводчик RU → EN (Helsinki-NLP/opus-mt-ru-en)",
                Category = "Переводчик",
                Description = "Локальный перевод русских промптов на английский без интернета и ограничений (~311 МБ).",
                SizeBytes = 312781498L,
                IsSelected = true
            };
            tr.Files.Add(new FileSpec { RelativePath = @"translate\opus-mt-ru-en\pytorch_model.bin", Url = "https://huggingface.co/Helsinki-NLP/opus-mt-ru-en/resolve/main/pytorch_model.bin", ExpectedSize = 306991893L, Description = "Веса модели перевода" });
            tr.Files.Add(new FileSpec { RelativePath = @"translate\opus-mt-ru-en\config.json", Url = "https://huggingface.co/Helsinki-NLP/opus-mt-ru-en/resolve/main/config.json", ExpectedSize = 1337L, Description = "Конфиг модели перевода" });
            tr.Files.Add(new FileSpec { RelativePath = @"translate\opus-mt-ru-en\generation_config.json", Url = "https://huggingface.co/Helsinki-NLP/opus-mt-ru-en/resolve/main/generation_config.json", ExpectedSize = 304L, Description = "Параметры генерации" });
            tr.Files.Add(new FileSpec { RelativePath = @"translate\opus-mt-ru-en\source.spm", Url = "https://huggingface.co/Helsinki-NLP/opus-mt-ru-en/resolve/main/source.spm", ExpectedSize = 1080169L, Description = "Токенизатор источника" });
            tr.Files.Add(new FileSpec { RelativePath = @"translate\opus-mt-ru-en\target.spm", Url = "https://huggingface.co/Helsinki-NLP/opus-mt-ru-en/resolve/main/target.spm", ExpectedSize = 802781L, Description = "Токенизатор цели" });
            tr.Files.Add(new FileSpec { RelativePath = @"translate\opus-mt-ru-en\tokenizer_config.json", Url = "https://huggingface.co/Helsinki-NLP/opus-mt-ru-en/resolve/main/tokenizer_config.json", ExpectedSize = 42L, Description = "Конфиг токенизатора" });
            tr.Files.Add(new FileSpec { RelativePath = @"translate\opus-mt-ru-en\vocab.json", Url = "https://huggingface.co/Helsinki-NLP/opus-mt-ru-en/resolve/main/vocab.json", ExpectedSize = 2601758L, Description = "Словарь" });
            catalog.Add(tr);

            // 2. Upscaler Models (RealESRGAN)
            var up = new ModelItem
            {
                Title = "Модели Апскейлера 2×/4× (RealESRGAN x4plus + Anime 6B)",
                Category = "Апскейлер",
                Description = "Нейросети сверхчеткого увеличения изображений в 2-4 раза: фото и аниме/арт (~85 МБ).",
                SizeBytes = 84796635L,
                IsSelected = true
            };
            up.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\upscale_models\RealESRGAN_x4plus.safetensors", Url = "https://huggingface.co/Comfy-Org/Real-ESRGAN_repackaged/resolve/main/RealESRGAN_x4plus.safetensors", ExpectedSize = 66857836L, Description = "RealESRGAN x4plus (Фото)" });
            up.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\upscale_models\RealESRGAN_x4plus_anime_6B.pth", Url = "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.2.4/RealESRGAN_x4plus_anime_6B.pth", ExpectedSize = 17938799L, Description = "RealESRGAN x4plus Anime 6B (Аниме)" });
            catalog.Add(up);

            // 3. Z-Image Ultimate NSFW v2.0
            var zimg = new ModelItem
            {
                Title = "AI Модель: Z-Image Ultimate NSFW v2.0 (Модель + CLIP + VAE)",
                Category = "AI Генерация",
                Description = "Сверхбыстрая генерация и фото-правка за 8 шагов (Civitai, ~17.0 ГБ).",
                SizeBytes = 12309876360L + 5631994051L + 335304388L,
                IsSelected = true,
                RegistryKey = "zImageUltimateNSFW_v20.safetensors",
                RegistryFamily = "zimage"
            };
            zimg.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\diffusion_models\zImageUltimateNSFW_v20.safetensors", Url = "https://civitai.com/api/download/models/2827368", ExpectedSize = 12309876360L, Description = "Z-Image Ultimate NSFW v2.0 Diffusion Model (11.5 ГБ)" });
            zimg.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\text_encoders\qwen_3_4b_fp8_mixed.safetensors", Url = "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/text_encoders/qwen_3_4b_fp8_mixed.safetensors", ExpectedSize = 5631994051L, Description = "Qwen 3.4B Text Encoder (5.25 ГБ)" });
            zimg.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\vae\ae.safetensors", Url = "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/vae/ae.safetensors", ExpectedSize = 335304388L, Description = "Z-Image Autoencoder VAE (320 МБ)" });
            catalog.Add(zimg);

            // 4. NoctQ V4 int8 (Qwen-Image 2.1)
            var noct = new ModelItem
            {
                Title = "AI Модель: NoctQ V4 int8 (Qwen-Image 2.1) (Модель + CLIP + VAE)",
                Category = "AI Генерация",
                Description = "Умная правка фото по инструкциям и детализированная генерация (Noctaluna, ~16.1 ГБ).",
                SizeBytes = 7256784368L + 9350798360L + 675509688L,
                IsSelected = true,
                RegistryKey = "NoctQ_V4_int8_convrot.safetensors",
                RegistryFamily = "qwen21"
            };
            noct.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\diffusion_models\NoctQ_V4_int8_convrot.safetensors", Url = "https://huggingface.co/Noctaluna/Noct-Q-Uncensored-Qwen-Image-2.1/resolve/main/NoctQ_V4_int8_convrot.safetensors", ExpectedSize = 7256784368L, Description = "NoctQ V4 INT8 Diffusion Model (6.76 ГБ)" });
            noct.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\text_encoders\qwen3vl_8b_int8_convrot.safetensors", Url = "https://huggingface.co/Noctaluna/Noct-Q-Uncensored-Qwen-Image-2.1/resolve/main/qwen3vl_8b_int8_convrot.safetensors", ExpectedSize = 9350798360L, Description = "Qwen 3VL 8B Text Encoder (8.71 ГБ)" });
            noct.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\vae\qwen_image_2.1_vae_bf16.safetensors", Url = "https://huggingface.co/Noctaluna/Noct-Q-Uncensored-Qwen-Image-2.1/resolve/main/qwen_image_2.1_vae_bf16.safetensors", ExpectedSize = 675509688L, Description = "Qwen-Image 2.1 VAE BF16 (644 МБ)" });
            catalog.Add(noct);
        }

        private void InitUI()
        {
            Text = "Студия — Мастер установки моделей и компонентов";
            Size = new Size(820, 720);
            MinimumSize = new Size(740, 620);
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Color.FromArgb(20, 21, 25);
            ForeColor = Color.FromArgb(240, 240, 245);
            Font = new Font("Segoe UI", 9.5f);

            try
            {
                string ico = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, @"ui\studio.ico");
                if (File.Exists(ico)) Icon = new Icon(ico);
            }
            catch { }

            var pnlTop = new Panel { Dock = DockStyle.Top, Height = 75, BackColor = Color.FromArgb(26, 28, 34), Padding = new Padding(22, 12, 22, 10) };
            var lblTitle = new Label { Text = "✦ Установщик Студии (ComfyUI AI Studio)", Font = new Font("Segoe UI", 14f, FontStyle.Bold), ForeColor = Color.White, AutoSize = true, Location = new Point(20, 12) };
            var lblSub = new Label { Text = "Автоматическая загрузка AI моделей генерации, апскейлеров и офлайн-переводчика", ForeColor = Color.FromArgb(160, 163, 175), AutoSize = true, Location = new Point(22, 42) };
            pnlTop.Controls.Add(lblTitle);
            pnlTop.Controls.Add(lblSub);
            Controls.Add(pnlTop);

            var pnlBody = new Panel { Dock = DockStyle.Fill, Padding = new Padding(22, 14, 22, 14) };

            // Directory selection
            var lblPath = new Label { Text = "Папка установки (корень проекта Студии / ComfyUI):", Location = new Point(20, 10), AutoSize = true, Font = new Font("Segoe UI", 9.5f, FontStyle.Bold) };
            txtTarget = new TextBox { Location = new Point(20, 34), Width = 640, Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right, BackColor = Color.FromArgb(32, 34, 42), ForeColor = Color.White, BorderStyle = BorderStyle.FixedSingle, Font = new Font("Segoe UI", 10f) };
            txtTarget.TextChanged += (s, e) => CheckTargetFolder();

            btnBrowse = new Button { Text = "Обзор...", Location = new Point(670, 32), Width = 100, Height = 28, Anchor = AnchorStyles.Top | AnchorStyles.Right, BackColor = Color.FromArgb(45, 48, 60), ForeColor = Color.White, FlatStyle = FlatStyle.Flat, Cursor = Cursors.Hand };
            btnBrowse.FlatAppearance.BorderColor = Color.FromArgb(65, 70, 85);
            btnBrowse.Click += (s, e) => BrowseFolder();

            lblDetect = new Label { Location = new Point(20, 68), AutoSize = true, ForeColor = Color.FromArgb(120, 210, 140) };

            // Component selection
            var lblSelect = new Label { Text = "Выберите компоненты и модели для загрузки и установки:", Location = new Point(20, 96), AutoSize = true, Font = new Font("Segoe UI", 9.5f, FontStyle.Bold) };
            clbModels = new CheckedListBox { Location = new Point(20, 122), Width = 750, Height = 140, Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right, BackColor = Color.FromArgb(28, 30, 38), ForeColor = Color.White, BorderStyle = BorderStyle.FixedSingle, CheckOnClick = true, Font = new Font("Segoe UI", 9.5f) };

            foreach (var item in catalog)
            {
                string text = string.Format("{0} [{1}]", item.Title, item.DisplaySize);
                clbModels.Items.Add(text, item.IsSelected);
            }
            clbModels.ItemCheck += (s, e) =>
            {
                catalog[e.Index].IsSelected = (e.NewValue == CheckState.Checked);
                UpdateTotalSizeLabel();
            };

            chkShortcut = new CheckBox { Text = "Создать удобный ярлык «Студия» на Рабочем столе с иконкой", Location = new Point(20, 270), AutoSize = true, Checked = true, ForeColor = Color.FromArgb(220, 222, 230) };

            // Progress section
            lblStatus = new Label { Text = "Готов к установке. Выберите компоненты и нажмите «Начать установку».", Location = new Point(20, 305), AutoSize = true, ForeColor = Color.FromArgb(190, 195, 205) };
            lblSpeed = new Label { Text = "", Location = new Point(600, 305), Width = 170, TextAlign = ContentAlignment.TopRight, Anchor = AnchorStyles.Top | AnchorStyles.Right, ForeColor = Color.FromArgb(100, 180, 255) };

            pbFile = new ProgressBar { Location = new Point(20, 330), Width = 750, Height = 12, Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right };
            pbTotal = new ProgressBar { Location = new Point(20, 348), Width = 750, Height = 18, Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right };

            // Log output
            rtbLog = new RichTextBox { Location = new Point(20, 375), Width = 750, Height = 180, Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right, BackColor = Color.FromArgb(14, 15, 18), ForeColor = Color.FromArgb(190, 200, 210), BorderStyle = BorderStyle.None, ReadOnly = true, Font = new Font("Consolas", 9f) };

            pnlBody.Controls.Add(lblPath);
            pnlBody.Controls.Add(txtTarget);
            pnlBody.Controls.Add(btnBrowse);
            pnlBody.Controls.Add(lblDetect);
            pnlBody.Controls.Add(lblSelect);
            pnlBody.Controls.Add(clbModels);
            pnlBody.Controls.Add(chkShortcut);
            pnlBody.Controls.Add(lblStatus);
            pnlBody.Controls.Add(lblSpeed);
            pnlBody.Controls.Add(pbFile);
            pnlBody.Controls.Add(pbTotal);
            pnlBody.Controls.Add(rtbLog);

            Controls.Add(pnlBody);

            // Bottom action panel
            var pnlBottom = new Panel { Dock = DockStyle.Bottom, Height = 65, BackColor = Color.FromArgb(26, 28, 34), Padding = new Padding(22, 14, 22, 14) };

            btnInstall = new Button { Text = "Начать установку", Location = new Point(460, 14), Width = 170, Height = 36, Anchor = AnchorStyles.Top | AnchorStyles.Right, BackColor = Color.FromArgb(41, 151, 255), ForeColor = Color.White, FlatStyle = FlatStyle.Flat, Font = new Font("Segoe UI", 10f, FontStyle.Bold), Cursor = Cursors.Hand };
            btnInstall.FlatAppearance.BorderSize = 0;
            btnInstall.Click += (s, e) => StartInstallation();

            btnCancel = new Button { Text = "Отмена", Location = new Point(645, 14), Width = 125, Height = 36, Anchor = AnchorStyles.Top | AnchorStyles.Right, BackColor = Color.FromArgb(48, 50, 60), ForeColor = Color.White, FlatStyle = FlatStyle.Flat, Cursor = Cursors.Hand };
            btnCancel.FlatAppearance.BorderSize = 0;
            btnCancel.Click += (s, e) => CancelOrExit();

            btnLaunch = new Button { Text = "✦ Запустить Студию", Location = new Point(20, 14), Width = 190, Height = 36, Visible = false, BackColor = Color.FromArgb(48, 209, 88), ForeColor = Color.White, FlatStyle = FlatStyle.Flat, Font = new Font("Segoe UI", 10f, FontStyle.Bold), Cursor = Cursors.Hand };
            btnLaunch.FlatAppearance.BorderSize = 0;
            btnLaunch.Click += (s, e) => LaunchStudio();

            pnlBottom.Controls.Add(btnLaunch);
            pnlBottom.Controls.Add(btnInstall);
            pnlBottom.Controls.Add(btnCancel);
            Controls.Add(pnlBottom);

            worker = new BackgroundWorker { WorkerReportsProgress = true, WorkerSupportsCancellation = true };
            worker.DoWork += Worker_DoWork;
            worker.ProgressChanged += Worker_ProgressChanged;
            worker.RunWorkerCompleted += Worker_RunWorkerCompleted;
        }

        private void DetectAppDirectory()
        {
            string baseDir = AppDomain.CurrentDomain.BaseDirectory.TrimEnd('\\', '/');

            // If running from installer subdirectory, step up
            if (Directory.Exists(Path.Combine(baseDir, @"..\ComfyUI")))
                baseDir = Path.GetFullPath(Path.Combine(baseDir, @".."));

            // Check if current directory is ComfyUI root
            if (File.Exists(Path.Combine(baseDir, @"ComfyUI\main.py")) || Directory.Exists(Path.Combine(baseDir, "ui")))
            {
                appRoot = baseDir;
            }
            else if (Directory.Exists(@"E:\ComfyUI"))
            {
                appRoot = @"E:\ComfyUI";
            }
            else
            {
                appRoot = baseDir;
            }

            txtTarget.Text = appRoot;
            CheckTargetFolder();
        }

        private void CheckTargetFolder()
        {
            string target = txtTarget.Text.Trim();
            if (string.IsNullOrEmpty(target) || !Directory.Exists(target))
            {
                lblDetect.Text = "Папка не существует (будет создана при установке).";
                lblDetect.ForeColor = Color.FromArgb(240, 180, 80);
                return;
            }

            bool hasComfy = File.Exists(Path.Combine(target, @"ComfyUI\main.py"));
            bool hasPy = File.Exists(Path.Combine(target, @"python_embeded\python.exe"));
            bool hasUI = Directory.Exists(Path.Combine(target, "ui"));

            if (hasComfy && hasPy && hasUI)
            {
                lblDetect.Text = "✓ Найдена рабочая среда Студии и ComfyUI.";
                lblDetect.ForeColor = Color.FromArgb(120, 220, 140);
            }
            else if (hasComfy)
            {
                lblDetect.Text = "✓ Обнаружен ComfyUI в указанной папке.";
                lblDetect.ForeColor = Color.FromArgb(120, 220, 140);
            }
            else
            {
                lblDetect.Text = "Внимание: ComfyUI не найден в этой папке. Модели будут помещены в стандартную структуру папок.";
                lblDetect.ForeColor = Color.FromArgb(240, 180, 80);
            }
        }

        private void BrowseFolder()
        {
            using (var fbd = new FolderBrowserDialog())
            {
                fbd.Description = "Выберите папку проекта Студии (где расположен ComfyUI)";
                fbd.SelectedPath = txtTarget.Text;
                if (fbd.ShowDialog() == DialogResult.OK)
                {
                    txtTarget.Text = fbd.SelectedPath;
                    CheckTargetFolder();
                }
            }
        }

        private void UpdateTotalSizeLabel()
        {
            long total = 0;
            foreach (var item in catalog)
            {
                if (item.IsSelected) total += item.SizeBytes;
            }
            lblStatus.Text = string.Format("Выбрано к установке: {0:0.1} ГБ", (double)total / (1024.0 * 1024 * 1024));
        }

        private void Log(string msg)
        {
            if (InvokeRequired)
            {
                Invoke(new Action<string>(Log), msg);
                return;
            }
            rtbLog.AppendText(string.Format("[{0}] {1}\r\n", DateTime.Now.ToString("HH:mm:ss"), msg));
            rtbLog.SelectionStart = rtbLog.Text.Length;
            rtbLog.ScrollToCaret();
        }

        private void StartInstallation()
        {
            string root = txtTarget.Text.Trim();
            if (string.IsNullOrEmpty(root))
            {
                MessageBox.Show("Укажите папку для установки.", "Студия", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            Directory.CreateDirectory(root);

            btnInstall.Enabled = false;
            btnBrowse.Enabled = false;
            txtTarget.Enabled = false;
            clbModels.Enabled = false;
            chkShortcut.Enabled = false;
            btnCancel.Text = "Прервать";

            cancelRequested = false;
            worker.RunWorkerAsync(root);
        }

        private void CancelOrExit()
        {
            if (worker.IsBusy)
            {
                if (MessageBox.Show("Прервать установку и скачивание файлов?", "Подтверждение", MessageBoxButtons.YesNo, MessageBoxIcon.Question) == DialogResult.Yes)
                {
                    cancelRequested = true;
                    worker.CancelAsync();
                    Log("Запрошена отмена установки...");
                }
            }
            else
            {
                Close();
            }
        }

        private void Worker_DoWork(object sender, DoWorkEventArgs e)
        {
            string root = (string)e.Argument;

            // Collect all files to download
            var files = new List<FileSpec>();
            var regUpdates = new Dictionary<string, string>();

            foreach (var item in catalog)
            {
                if (item.IsSelected)
                {
                    files.AddRange(item.Files);
                    if (!string.IsNullOrEmpty(item.RegistryKey) && !string.IsNullOrEmpty(item.RegistryFamily))
                    {
                        regUpdates[item.RegistryKey] = item.RegistryFamily;
                    }
                }
            }

            long totalBytesAll = 0;
            foreach (var f in files) totalBytesAll += f.ExpectedSize;

            long totalDoneBytes = 0;
            Log(string.Format("Начало установки: выбрано файлов {0}, общий объем: {1:0.1} ГБ", files.Count, (double)totalBytesAll / (1024.0 * 1024 * 1024)));

            byte[] buffer = new byte[65536];

            for (int i = 0; i < files.Count; i++)
            {
                if (cancelRequested) { e.Cancel = true; return; }

                var f = files[i];
                string destPath = Path.Combine(root, f.RelativePath);
                Directory.CreateDirectory(Path.GetDirectoryName(destPath));

                // Check if already exists with matching size
                if (File.Exists(destPath))
                {
                    var fi = new FileInfo(destPath);
                    if (fi.Length == f.ExpectedSize || (f.ExpectedSize == 0 && fi.Length > 0))
                    {
                        Log(string.Format("✓ Файл уже установлен (пропуск): {0} ({1:0.1} МБ)", Path.GetFileName(destPath), (double)fi.Length / (1024.0 * 1024)));
                        totalDoneBytes += f.ExpectedSize;
                        int totalPct = totalBytesAll > 0 ? (int)((totalDoneBytes * 100) / totalBytesAll) : 100;
                        worker.ReportProgress(totalPct, new ProgressInfo { CurrentFile = Path.GetFileName(destPath), FilePct = 100, SpeedStr = "уже на диске" });
                        continue;
                    }
                }

                Log(string.Format("↓ Скачивание [{0}/{1}]: {2}...", i + 1, files.Count, Path.GetFileName(destPath)));
                string partPath = destPath + ".part";

                long existingPart = 0;
                if (File.Exists(partPath))
                {
                    existingPart = new FileInfo(partPath).Length;
                    if (existingPart > f.ExpectedSize) { File.Delete(partPath); existingPart = 0; }
                }

                bool success = false;
                int retries = 0;

                while (!success && retries < 3)
                {
                    if (cancelRequested) { e.Cancel = true; return; }
                    try
                    {
                        var req = (HttpWebRequest)WebRequest.Create(f.Url);
                        req.UserAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
                        req.Timeout = 60000;
                        req.ReadWriteTimeout = 60000;
                        req.AllowAutoRedirect = true;

                        if (existingPart > 0)
                        {
                            req.AddRange(existingPart);
                            Log(string.Format("  Возобновление скачивания с позиции {0:0.1} МБ", (double)existingPart / (1024.0 * 1024)));
                        }

                        using (var resp = (HttpWebResponse)req.GetResponse())
                        using (var stream = resp.GetResponseStream())
                        using (var fs = new FileStream(partPath, existingPart > 0 ? FileMode.Append : FileMode.Create, FileAccess.Write, FileShare.None))
                        {
                            long fileTotal = f.ExpectedSize;
                            if (resp.ContentLength > 0 && existingPart == 0) fileTotal = resp.ContentLength;

                            long fileDownloaded = existingPart;
                            var sw = Stopwatch.StartNew();
                            long lastBytes = fileDownloaded;
                            long lastTime = sw.ElapsedMilliseconds;

                            int read;
                            while ((read = stream.Read(buffer, 0, buffer.Length)) > 0)
                            {
                                if (cancelRequested) { e.Cancel = true; return; }

                                fs.Write(buffer, 0, read);
                                fileDownloaded += read;
                                totalDoneBytes += read;

                                long now = sw.ElapsedMilliseconds;
                                if (now - lastTime >= 500)
                                {
                                    double speedMB = ((fileDownloaded - lastBytes) / (1024.0 * 1024.0)) / ((now - lastTime) / 1000.0);
                                    lastBytes = fileDownloaded;
                                    lastTime = now;

                                    int filePct = fileTotal > 0 ? (int)((fileDownloaded * 100) / fileTotal) : 0;
                                    int totalPct = totalBytesAll > 0 ? (int)((totalDoneBytes * 100) / totalBytesAll) : 0;

                                    worker.ReportProgress(totalPct, new ProgressInfo
                                    {
                                        CurrentFile = Path.GetFileName(destPath),
                                        FilePct = Math.Min(100, filePct),
                                        SpeedStr = string.Format("{0:0.1} МБ/с · {1:0.1} / {2:0.1} МБ", speedMB, (double)fileDownloaded / (1024.0 * 1024), (double)fileTotal / (1024.0 * 1024))
                                    });
                                }
                            }
                        }

                        // Verify download and move .part to final
                        if (File.Exists(destPath)) File.Delete(destPath);
                        File.Move(partPath, destPath);
                        Log(string.Format("✓ Загружен: {0}", Path.GetFileName(destPath)));
                        success = true;
                    }
                    catch (Exception ex)
                    {
                        retries++;
                        Log(string.Format("! Ошибка при загрузке ({0}/3): {1}", retries, ex.Message));
                        if (retries >= 3) throw;
                        Thread.Sleep(3000);
                    }
                }
            }

            // Sync models.json
            if (regUpdates.Count > 0)
            {
                UpdateModelsRegistry(root, regUpdates);
            }

            // Desktop shortcut
            if (chkShortcut.Checked)
            {
                CreateDesktopShortcut(root);
            }
        }

        private void UpdateModelsRegistry(string root, Dictionary<string, string> updates)
        {
            try
            {
                string regPath = Path.Combine(root, @"ui\models.json");
                var current = new Dictionary<string, string>();
                if (File.Exists(regPath))
                {
                    string txt = File.ReadAllText(regPath, Encoding.UTF8);
                    // simple JSON parser for string-to-string dictionary
                    var matches = System.Text.RegularExpressions.Regex.Matches(txt, "\"([^\"]+)\"\\s*:\\s*\"([^\"]+)\"");
                    foreach (System.Text.RegularExpressions.Match m in matches)
                    {
                        current[m.Groups[1].Value] = m.Groups[2].Value;
                    }
                }

                foreach (var kvp in updates)
                {
                    current[kvp.Key] = kvp.Value;
                }

                var sb = new StringBuilder();
                sb.AppendLine("{");
                int idx = 0;
                foreach (var kvp in current)
                {
                    sb.AppendFormat(" \"{0}\": \"{1}\"{2}\r\n", kvp.Key, kvp.Value, ++idx < current.Count ? "," : "");
                }
                sb.AppendLine("}");

                Directory.CreateDirectory(Path.GetDirectoryName(regPath));
                File.WriteAllText(regPath, sb.ToString(), Encoding.UTF8);
                Log("✓ Обновлен реестр моделей: ui\\models.json");
            }
            catch (Exception ex)
            {
                Log("! Предупреждение: не удалось обновить models.json: " + ex.Message);
            }
        }

        private void CreateDesktopShortcut(string root)
        {
            try
            {
                string desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
                string lnkPath = Path.Combine(desktop, "Студия.lnk");
                string icoPath = Path.Combine(root, @"ui\studio.ico");
                string exeApp = Path.Combine(root, @"app\Studio.exe");
                string ps1App = Path.Combine(root, "Studio.ps1");

                Type shellType = Type.GetTypeFromProgID("WScript.Shell");
                if (shellType != null)
                {
                    object shell = Activator.CreateInstance(shellType);
                    object shortcut = shellType.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, shell, new object[] { lnkPath });

                    if (File.Exists(exeApp))
                    {
                        shortcut.GetType().InvokeMember("TargetPath", BindingFlags.SetProperty, null, shortcut, new object[] { exeApp });
                    }
                    else
                    {
                        string powershell = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), @"WindowsPowerShell\v1.0\powershell.exe");
                        shortcut.GetType().InvokeMember("TargetPath", BindingFlags.SetProperty, null, shortcut, new object[] { powershell });
                        shortcut.GetType().InvokeMember("Arguments", BindingFlags.SetProperty, null, shortcut, new object[] { string.Format("-ExecutionPolicy Bypass -WindowStyle Hidden -File \"{0}\"", ps1App) });
                    }

                    shortcut.GetType().InvokeMember("WorkingDirectory", BindingFlags.SetProperty, null, shortcut, new object[] { root });
                    if (File.Exists(icoPath))
                    {
                        shortcut.GetType().InvokeMember("IconLocation", BindingFlags.SetProperty, null, shortcut, new object[] { icoPath + ",0" });
                    }
                    shortcut.GetType().InvokeMember("Description", BindingFlags.SetProperty, null, shortcut, new object[] { "Студия AI: Генерация, правка фото и апскейл" });
                    shortcut.GetType().InvokeMember("Save", BindingFlags.InvokeMethod, null, shortcut, null);

                    Log("✓ Ярлык «Студия» успешно создан на Рабочем столе.");
                }
            }
            catch (Exception ex)
            {
                Log("! Предупреждение: не удалось создать ярлык: " + ex.Message);
            }
        }

        private void Worker_ProgressChanged(object sender, ProgressChangedEventArgs e)
        {
            pbTotal.Value = Math.Max(0, Math.Min(100, e.ProgressPercentage));
            var info = e.UserState as ProgressInfo;
            if (info != null)
            {
                pbFile.Value = Math.Max(0, Math.Min(100, info.FilePct));
                lblStatus.Text = string.Format("Установка: {0} ({1}%)", info.CurrentFile, info.FilePct);
                lblSpeed.Text = info.SpeedStr;
            }
        }

        private void Worker_RunWorkerCompleted(object sender, RunWorkerCompletedEventArgs e)
        {
            btnInstall.Enabled = true;
            btnBrowse.Enabled = true;
            txtTarget.Enabled = true;
            clbModels.Enabled = true;
            chkShortcut.Enabled = true;
            btnCancel.Text = "Закрыть";

            if (e.Cancelled)
            {
                lblStatus.Text = "Установка прервана пользователем.";
                lblSpeed.Text = "";
                Log("Установка отменена.");
            }
            else if (e.Error != null)
            {
                lblStatus.Text = "Ошибка установки: " + e.Error.Message;
                lblStatus.ForeColor = Color.FromArgb(255, 90, 80);
                lblSpeed.Text = "";
                Log("ОШИБКА: " + e.Error.ToString());
                MessageBox.Show("Произошла ошибка при установке: " + e.Error.Message, "Ошибка", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
            else
            {
                pbTotal.Value = 100;
                pbFile.Value = 100;
                lblStatus.Text = "✓ Установка успешно завершена! Всё готово к работе.";
                lblStatus.ForeColor = Color.FromArgb(48, 209, 88);
                lblSpeed.Text = "";
                Log("✓ ВСЕ ВЫБРАННЫЕ КОМПОНЕНТЫ УСПЕШНО УСТАНОВЛЕНЫ!");
                btnLaunch.Visible = true;
                btnInstall.Visible = false;
            }
        }

        private void LaunchStudio()
        {
            string root = txtTarget.Text.Trim();
            string exe = Path.Combine(root, @"app\Studio.exe");
            string bat = Path.Combine(root, "start_studio.bat");
            string ps1 = Path.Combine(root, "Studio.ps1");

            try
            {
                if (File.Exists(exe))
                {
                    Process.Start(new ProcessStartInfo(exe) { WorkingDirectory = root });
                }
                else if (File.Exists(ps1))
                {
                    Process.Start(new ProcessStartInfo("powershell.exe", string.Format("-ExecutionPolicy Bypass -File \"{0}\"", ps1)) { WorkingDirectory = root });
                }
                else if (File.Exists(bat))
                {
                    Process.Start(new ProcessStartInfo(bat) { WorkingDirectory = root });
                }
                Close();
            }
            catch (Exception ex)
            {
                MessageBox.Show("Не удалось запустить: " + ex.Message, "Ошибка запуска", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
    }

    public class ProgressInfo
    {
        public string CurrentFile { get; set; }
        public int FilePct { get; set; }
        public string SpeedStr { get; set; }
    }
}
