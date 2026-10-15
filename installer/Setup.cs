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
        public bool NeedsToken { get; set; }
        public bool InstallGgufNode { get; set; }
        public bool InstallLipsyncNode { get; set; }

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
        public bool Gated { get; set; }
    }

    public class MainForm : Form
    {
        private TextBox txtTarget;
        private Button btnBrowse;
        private Label lblDetect;
        private CheckedListBox clbModels;
        private CheckBox chkShortcut;
        private TextBox txtToken;
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

            // 2. Upscaler Models (Phips: Nomos2 hq dat2 for photos, HFA2k for anime)
            var up = new ModelItem
            {
                Title = "Модели Апскейлера ×2 (Nomos2 hq dat2 + HFA2k)",
                Category = "Апскейлер",
                Description = "Нейросети сверхчеткого увеличения изображений в 2-4 раза: фото, люди, предметы и аниме/арт (~173 МБ).",
                SizeBytes = 173260410L,
                IsSelected = true
            };
            up.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\upscale_models\4xNomos2_hq_dat2.safetensors", Url = "https://huggingface.co/Phips/4xNomos2_hq_dat2/resolve/main/4xNomos2_hq_dat2.safetensors", ExpectedSize = 139792588L, Description = "4xNomos2 hq dat2 (Фото)" });
            up.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\upscale_models\4xHFA2k.safetensors", Url = "https://huggingface.co/Phips/4xHFA2k/resolve/main/4xHFA2k.safetensors", ExpectedSize = 33467822L, Description = "4xHFA2k (Аниме)" });
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

            // 5. LTX-2.5 video (GGUF). Only the transformer is open; everything else needs a Hugging Face account that accepted the LTX license.
            const string lt = "https://huggingface.co/Lightricks/LTX-2.5/resolve/main/";
            var ltx = new ModelItem
            {
                Title = "Видео: LTX-2.5 (GGUF, влезает в 16 ГБ) + ComfyUI-GGUF",
                Category = "Видео",
                Description = "Текст → видео и фото → видео со звуком (~34 ГБ). Нужен токен Hugging Face и принятая лицензия Lightricks/LTX-2.5.",
                SizeBytes = 15687639488L + 15372969374L + 1472223346L + 364866540L + 995778752L,
                IsSelected = false,
                NeedsToken = true,
                InstallGgufNode = true
            };
            ltx.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\unet\LTX-2.5-Distilled-Q4_K_M.gguf", Url = "https://huggingface.co/Abiray/LTX-2.5-Distilled-GGUF/resolve/main/LTX-2.5-Distilled-Q4_K_M.gguf", ExpectedSize = 15687639488L, Description = "LTX-2.5 Distilled Q4_K_M (14.6 ГБ)" });
            ltx.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\text_encoders\gemma4-12b-with-proj-ltx-2.5-comfy-int8-convrot.safetensors", Url = lt + "text_encoders/gemma4-12b-with-proj-ltx-2.5-comfy-int8-convrot.safetensors", ExpectedSize = 15372969374L, Description = "Gemma 4 12B энкодер int8 (14.3 ГБ)", Gated = true });
            ltx.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\vae\ltx-2.5-video-vae-bf16.safetensors", Url = lt + "vae/ltx-2.5-video-vae-bf16.safetensors", ExpectedSize = 1472223346L, Description = "LTX-2.5 видео VAE (1.4 ГБ)", Gated = true });
            ltx.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\vae\ltx-2.5-audio-vae-bf16.safetensors", Url = lt + "vae/ltx-2.5-audio-vae-bf16.safetensors", ExpectedSize = 364866540L, Description = "LTX-2.5 аудио VAE (348 МБ)", Gated = true });
            ltx.Files.Add(new FileSpec { RelativePath = @"ComfyUI\models\latent_upscale_models\ltx-2.5-latent-spatial-upscaler-x2-bf16-1.0.safetensors", Url = lt + "latent_upscale_models/ltx-2.5-latent-spatial-upscaler-x2-bf16-1.0.safetensors", ExpectedSize = 995778752L, Description = "Апскейлер латентов ×2 (950 МБ)", Gated = true });
            catalog.Add(ltx);

            // 6. Lip sync: LatentSync 1.6 (ByteDance) with its Stable Diffusion VAE and Whisper-tiny audio encoder; all open, no token needed.
            const string ls = "https://huggingface.co/ByteDance/LatentSync-1.6/resolve/main/";
            const string lsDir = @"ComfyUI\custom_nodes\ComfyUI-LatentSyncWrapper\checkpoints\";
            var lip = new ModelItem
            {
                Title = "Липсинк: LatentSync 1.6 (фото или видео + аудио → видео с говорящим лицом) + нода",
                Category = "Липсинк",
                Description = "Синхронизация губ по аудио, полностью локально (~5.5 ГБ). Нужны видеокарта от 16 ГБ и ffmpeg в PATH.",
                SizeBytes = 5072222488L + 75572083L + 334643276L + 547L,
                IsSelected = false,
                InstallLipsyncNode = true
            };
            lip.Files.Add(new FileSpec { RelativePath = lsDir + "latentsync_unet.pt", Url = ls + "latentsync_unet.pt", ExpectedSize = 5072222488L, Description = "LatentSync 1.6 UNet (4.7 ГБ)" });
            lip.Files.Add(new FileSpec { RelativePath = lsDir + @"whisper\tiny.pt", Url = ls + "whisper/tiny.pt", ExpectedSize = 75572083L, Description = "Whisper tiny, аудио-энкодер (72 МБ)" });
            lip.Files.Add(new FileSpec { RelativePath = lsDir + @"vae\diffusion_pytorch_model.safetensors", Url = "https://huggingface.co/stabilityai/sd-vae-ft-mse/resolve/main/diffusion_pytorch_model.safetensors", ExpectedSize = 334643276L, Description = "Stable Diffusion VAE (319 МБ)" });
            lip.Files.Add(new FileSpec { RelativePath = lsDir + @"vae\config.json", Url = "https://huggingface.co/stabilityai/sd-vae-ft-mse/resolve/main/config.json", ExpectedSize = 547L, Description = "Конфиг VAE" });
            catalog.Add(lip);
        }

        private void InitUI()
        {
            Text = "Студия — Мастер установки моделей и компонентов";
            Size = new Size(820, 770);
            MinimumSize = new Size(740, 670);
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
            var lblSub = new Label { Text = "Автоматическая загрузка AI моделей: фото, видео, липсинк, апскейлеры и офлайн-переводчик", ForeColor = Color.FromArgb(160, 163, 175), AutoSize = true, Location = new Point(22, 42) };
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

            var lblToken = new Label { Text = "Токен Hugging Face (только для видео LTX-2.5, нигде не сохраняется):", Location = new Point(20, 298), AutoSize = true, ForeColor = Color.FromArgb(190, 195, 205) };
            txtToken = new TextBox { Location = new Point(20, 320), Width = 750, Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right, BackColor = Color.FromArgb(32, 34, 42), ForeColor = Color.White, BorderStyle = BorderStyle.FixedSingle, UseSystemPasswordChar = true };

            // Progress section
            lblStatus = new Label { Text = "Готов к установке. Выберите компоненты и нажмите «Начать установку».", Location = new Point(20, 355), AutoSize = true, ForeColor = Color.FromArgb(190, 195, 205) };
            lblSpeed = new Label { Text = "", Location = new Point(600, 355), Width = 170, TextAlign = ContentAlignment.TopRight, Anchor = AnchorStyles.Top | AnchorStyles.Right, ForeColor = Color.FromArgb(100, 180, 255) };

            pbFile = new ProgressBar { Location = new Point(20, 380), Width = 750, Height = 12, Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right };
            pbTotal = new ProgressBar { Location = new Point(20, 398), Width = 750, Height = 18, Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right };

            // Log output
            rtbLog = new RichTextBox { Location = new Point(20, 425), Width = 750, Height = 180, Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right, BackColor = Color.FromArgb(14, 15, 18), ForeColor = Color.FromArgb(190, 200, 210), BorderStyle = BorderStyle.None, ReadOnly = true, Font = new Font("Consolas", 9f) };

            pnlBody.Controls.Add(lblPath);
            pnlBody.Controls.Add(txtTarget);
            pnlBody.Controls.Add(btnBrowse);
            pnlBody.Controls.Add(lblDetect);
            pnlBody.Controls.Add(lblSelect);
            pnlBody.Controls.Add(clbModels);
            pnlBody.Controls.Add(chkShortcut);
            pnlBody.Controls.Add(lblToken);
            pnlBody.Controls.Add(txtToken);
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
            txtToken.Enabled = false;
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
            bool installGguf = false;
            bool installLipsync = false;

            foreach (var item in catalog)
            {
                if (item.IsSelected)
                {
                    files.AddRange(item.Files);
                    if (item.InstallGgufNode) installGguf = true;
                    if (item.InstallLipsyncNode) installLipsync = true;
                    if (!string.IsNullOrEmpty(item.RegistryKey) && !string.IsNullOrEmpty(item.RegistryFamily))
                    {
                        regUpdates[item.RegistryKey] = item.RegistryFamily;
                    }
                }
            }

            string hfToken = null;
            FileSpec firstGated = null;
            foreach (var f in files)
            {
                if (f.Gated && !File.Exists(Path.Combine(root, f.RelativePath))) { firstGated = f; break; }
            }
            if (firstGated != null)
            {
                hfToken = ResolveToken();
                if (string.IsNullOrEmpty(hfToken))
                    throw new Exception("Для видео LTX-2.5 нужен токен Hugging Face (права Read). Примите лицензию на https://huggingface.co/Lightricks/LTX-2.5 и вставьте токен в поле установщика.");
                CheckGatedAccess(firstGated.Url, hfToken);
                Log("✓ Доступ к Lightricks/LTX-2.5 подтверждён.");
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
                        if (existingPart > 0)
                            Log(string.Format("  Возобновление скачивания с позиции {0:0.1} МБ", (double)existingPart / (1024.0 * 1024)));

                        using (var resp = OpenResponse(f.Url, existingPart, f.Gated ? hfToken : null))
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

            if (installGguf)
            {
                InstallGgufNode(root);
            }

            if (installLipsync)
            {
                InstallLipsyncNode(root);
            }

            // Desktop shortcut
            if (chkShortcut.Checked)
            {
                CreateDesktopShortcut(root);
            }
        }

        // The token goes only to huggingface.co: redirects are followed by hand so it is never sent on to a CDN host.
        private HttpWebResponse OpenResponse(string url, long from, string token)
        {
            for (int hop = 0; hop < 6; hop++)
            {
                var req = (HttpWebRequest)WebRequest.Create(url);
                req.UserAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
                req.Timeout = 60000;
                req.ReadWriteTimeout = 60000;
                req.AllowAutoRedirect = false;
                if (from > 0) req.AddRange(from);
                if (!string.IsNullOrEmpty(token) && req.RequestUri.Host.EndsWith("huggingface.co", StringComparison.OrdinalIgnoreCase))
                    req.Headers["Authorization"] = "Bearer " + token;

                HttpWebResponse resp;
                try { resp = (HttpWebResponse)req.GetResponse(); }
                catch (WebException ex)
                {
                    var er = ex.Response as HttpWebResponse;
                    if (er != null && !string.IsNullOrEmpty(token) && (er.StatusCode == HttpStatusCode.Unauthorized || er.StatusCode == HttpStatusCode.Forbidden))
                        throw new Exception("Hugging Face отказал в доступе (" + (int)er.StatusCode + "). Проверьте токен и примите лицензию на странице модели.");
                    throw;
                }
                int code = (int)resp.StatusCode;
                if (code >= 300 && code < 400)
                {
                    string loc = resp.Headers["Location"];
                    resp.Close();
                    if (string.IsNullOrEmpty(loc)) throw new Exception("Сервер вернул перенаправление без адреса.");
                    url = new Uri(req.RequestUri, loc).ToString();
                    continue;
                }
                return resp;
            }
            throw new Exception("Слишком много перенаправлений при загрузке.");
        }

        private string ResolveToken()
        {
            string t = null;
            Invoke(new Action(() => { t = txtToken.Text.Trim(); }));
            if (string.IsNullOrEmpty(t)) t = Environment.GetEnvironmentVariable("HF_TOKEN");
            if (string.IsNullOrEmpty(t))
            {
                // the file huggingface_hub.login() writes
                string f = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), @".cache\huggingface\token");
                if (File.Exists(f)) t = File.ReadAllText(f).Trim();
            }
            return t;
        }

        private void CheckGatedAccess(string url, string token)
        {
            using (var r = OpenResponse(url, 0, token)) { }   // throws a readable message on 401/403
        }

        private const string GgufNodeCommit = "6ea2651e7df66d7585f6ffee804b20e92fb38b8a";

        // ComfyUI-GGUF (pinned commit) into custom_nodes, loader.py patched for Gemma 4 / LTX (same change as patches\ComfyUI-GGUF-ltx25-gemma4.patch), then its Python deps.
        private void InstallGgufNode(string root)
        {
            try
            {
                string comfy = Path.Combine(root, "ComfyUI");
                string ver = Path.Combine(comfy, "comfyui_version.py");
                if (File.Exists(ver))
                {
                    var m = System.Text.RegularExpressions.Regex.Match(File.ReadAllText(ver), "\"0\\.(\\d+)\\.\\d+\"");
                    if (m.Success && int.Parse(m.Groups[1].Value) < 39)
                        Log("! ComfyUI старее 0.39: для LTX-2.5 обновите его (update\\update_comfyui.bat).");
                }

                string nodeDir = Path.Combine(comfy, @"custom_nodes\ComfyUI-GGUF");
                if (!File.Exists(Path.Combine(nodeDir, "loader.py")))
                {
                    Log("↓ ComfyUI-GGUF (GitHub, зафиксированная версия)...");
                    string tmp = Path.Combine(Path.GetTempPath(), "studio_gguf_" + Guid.NewGuid().ToString("N"));
                    string zip = tmp + ".zip";
                    using (var wc = new WebClient())
                    {
                        wc.Headers["User-Agent"] = "Mozilla/5.0";
                        wc.DownloadFile("https://github.com/city96/ComfyUI-GGUF/archive/" + GgufNodeCommit + ".zip", zip);
                    }
                    System.IO.Compression.ZipFile.ExtractToDirectory(zip, tmp);
                    Directory.CreateDirectory(Path.GetDirectoryName(nodeDir));
                    Directory.Move(Path.Combine(tmp, "ComfyUI-GGUF-" + GgufNodeCommit), nodeDir);
                    File.Delete(zip);
                    Directory.Delete(tmp, true);
                }

                string loader = Path.Combine(nodeDir, "loader.py");
                string src = File.ReadAllText(loader).Replace("\r\n", "\n");
                if (src.Contains("\"gemma4\""))
                {
                    Log("✓ ComfyUI-GGUF уже пропатчен.");
                }
                else
                {
                    string a1 = "TXT_ARCH_LIST = {\"t5\", \"t5encoder\", \"llama\", \"qwen2vl\", \"qwen3\", \"qwen3vl\", \"gemma3\"}";
                    string a2 = "VIS_TYPE_LIST = {\"clip-vision\", \"mmproj\"}\n";
                    string a3 = "        # 1D tensors shouldn't be quantized, this is a fix for BF16\n        if len(shape) <= 1 and tensor.tensor_type == gguf.GGMLQuantizationType.BF16:";
                    if (!src.Contains(a1) || !src.Contains(a2) || !src.Contains(a3))
                        throw new Exception("loader.py имеет неожиданный вид, патч не применён.");
                    src = src.Replace(a1, "TXT_ARCH_LIST = {\"t5\", \"t5encoder\", \"llama\", \"qwen2vl\", \"qwen3\", \"qwen3vl\", \"gemma3\", \"gemma4\"}");
                    src = src.Replace(a2, a2 + "LTXV_BF16_PARAMETERS = {\n    \"audio_embeddings_connector.learnable_registers\",\n    \"keyframes_abs_pos_embedding\",\n    \"video_embeddings_connector.learnable_registers\",\n}\n");
                    src = src.Replace(a3, "        # Raw LTXAV parameters are not loaded through GGMLOps, so their BF16 byte storage must be decoded here.\n        if tensor.tensor_type == gguf.GGMLQuantizationType.BF16 and (len(shape) <= 1 or (arch_str == \"ltxv\" and tensor_name in LTXV_BF16_PARAMETERS)):");
                    File.WriteAllText(loader, src);
                    Log("✓ ComfyUI-GGUF: применён патч для Gemma 4 / LTX.");
                }

                string py = Path.Combine(root, @"python_embeded\python.exe");
                if (File.Exists(py))
                {
                    Log("Установка Python-зависимостей ComfyUI-GGUF (gguf, sentencepiece, protobuf)...");
                    var psi = new ProcessStartInfo(py, "-s -m pip install \"gguf>=0.13.0\" sentencepiece protobuf")
                    {
                        WorkingDirectory = root, UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true
                    };
                    using (var pr = Process.Start(psi))
                    {
                        string o = pr.StandardOutput.ReadToEnd() + pr.StandardError.ReadToEnd();
                        pr.WaitForExit();
                        Log(pr.ExitCode == 0 ? "✓ Зависимости установлены." : "! pip завершился с ошибкой: " + o.Trim());
                    }
                }
                else Log("! python_embeded не найден: установите вручную gguf, sentencepiece, protobuf.");
            }
            catch (Exception ex)
            {
                Log("! Не удалось установить ComfyUI-GGUF: " + ex.Message);
            }
        }

        private const string LipsyncNodeCommit = "360d5283d7276aee68b4237b1387e594e4ce640e";
        // The tested wrapper needs two small fixes for current PyTorch/PyAV (see patches\ComfyUI-LatentSyncWrapper-local.patch);
        // the patched files live in this repository and replace the originals.
        private const string LipsyncPatchBase = "https://raw.githubusercontent.com/reyjendiz/studio-ai/main/patches/LatentSyncWrapper/";
        private const string LipsyncPackages = "diffusers==0.41.0 omegaconf==2.4.0 einops==0.8.2 mediapipe==1.1.1 face-alignment==1.5.0 decord==0.6.0 ffmpeg-python==0.2.0 soundfile==0.14.0 DeepCache==0.1.1 python_speech_features==0.6 scenedetect==0.5.6.1 kornia==0.8.3 opencv-python==5.0.0.93";

        private static void CopyTree(string from, string to)
        {
            Directory.CreateDirectory(to);
            foreach (string f in Directory.GetFiles(from)) File.Copy(f, Path.Combine(to, Path.GetFileName(f)), true);
            foreach (string d in Directory.GetDirectories(from)) CopyTree(d, Path.Combine(to, Path.GetFileName(d)));
        }

        // ComfyUI-LatentSyncWrapper (pinned commit) into custom_nodes next to the checkpoints that were just downloaded, the two patched
        // files over it, then its Python packages.
        private void InstallLipsyncNode(string root)
        {
            try
            {
                string nodeDir = Path.Combine(root, @"ComfyUI\custom_nodes\ComfyUI-LatentSyncWrapper");
                if (!File.Exists(Path.Combine(nodeDir, "nodes.py")))
                {
                    Log("↓ ComfyUI-LatentSyncWrapper (GitHub, зафиксированная версия)...");
                    string tmp = Path.Combine(Path.GetTempPath(), "studio_lipsync_" + Guid.NewGuid().ToString("N"));
                    string zip = tmp + ".zip";
                    using (var wc = new WebClient())
                    {
                        wc.Headers["User-Agent"] = "Mozilla/5.0";
                        wc.DownloadFile("https://github.com/ShmuelRonen/ComfyUI-LatentSyncWrapper/archive/" + LipsyncNodeCommit + ".zip", zip);
                    }
                    System.IO.Compression.ZipFile.ExtractToDirectory(zip, tmp);
                    CopyTree(Path.Combine(tmp, "ComfyUI-LatentSyncWrapper-" + LipsyncNodeCommit), nodeDir);
                    File.Delete(zip);
                    Directory.Delete(tmp, true);
                }

                foreach (string rel in new[] { "nodes.py", @"latentsync\utils\util.py" })
                {
                    string path = Path.Combine(nodeDir, rel);
                    if (File.Exists(path) && File.ReadAllText(path).Contains("local patch"))
                    {
                        Log("✓ LatentSync уже пропатчен: " + rel);
                        continue;
                    }
                    string text;
                    using (var wc = new WebClient())
                    {
                        wc.Headers["User-Agent"] = "Mozilla/5.0";
                        wc.Encoding = Encoding.UTF8;
                        text = wc.DownloadString(LipsyncPatchBase + rel.Replace('\\', '/'));
                    }
                    if (!text.Contains("local patch")) throw new Exception("получен неожиданный файл вместо патча: " + rel);
                    Directory.CreateDirectory(Path.GetDirectoryName(path));
                    File.WriteAllText(path, text, new UTF8Encoding(false));
                    Log("✓ LatentSync: применён патч " + rel);
                }

                string py = Path.Combine(root, @"python_embeded\python.exe");
                if (File.Exists(py))
                {
                    Log("Установка Python-пакетов LatentSync (несколько минут)...");
                    var psi = new ProcessStartInfo(py, "-s -m pip install " + LipsyncPackages)
                    {
                        WorkingDirectory = root, UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true
                    };
                    using (var pr = Process.Start(psi))
                    {
                        string o = pr.StandardOutput.ReadToEnd() + pr.StandardError.ReadToEnd();
                        pr.WaitForExit();
                        Log(pr.ExitCode == 0 ? "✓ Пакеты установлены." : "! pip завершился с ошибкой: " + o.Trim());
                    }
                }
                else Log("! python_embeded не найден: установите пакеты вручную: pip install " + LipsyncPackages);

                try
                {
                    var ff = Process.Start(new ProcessStartInfo("ffmpeg", "-version") { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true });
                    ff.WaitForExit();
                }
                catch { Log("! ffmpeg не найден в PATH: он нужен для липсинка (winget install Gyan.FFmpeg)."); }
            }
            catch (Exception ex)
            {
                Log("! Не удалось установить LatentSync: " + ex.Message);
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
                    string iconSource = File.Exists(exeApp) ? exeApp : icoPath;
                    if (File.Exists(iconSource))
                    {
                        shortcut.GetType().InvokeMember("IconLocation", BindingFlags.SetProperty, null, shortcut, new object[] { iconSource + ",0" });
                    }
                    shortcut.GetType().InvokeMember("Description", BindingFlags.SetProperty, null, shortcut, new object[] { "Студия AI: фото, видео и липсинк" });
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
            txtToken.Enabled = true;
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
