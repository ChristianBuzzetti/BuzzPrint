#nullable disable
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace BuzzPrint
{
    public class CaptureResult
    {
        public Bitmap Image;
        public string Source;
        public Rectangle Area;   // screen pixels
    }

    /// <summary>
    /// "Todo en uno" capture: freezes the screen, then either drag a region or click a window
    /// (the window under the cursor is highlighted). Esc cancels.
    /// </summary>
    public class CaptureOverlay : Form
    {
        readonly Bitmap screen, dim;
        readonly Rectangle virt;
        readonly List<KeyValuePair<IntPtr, Rectangle>> windows;
        Point mouse, start;
        bool dragging, moved;
        Rectangle sel, hover;
        IntPtr hoverHandle;
        Rectangle chosen;
        string chosenSource = "";
        readonly Font labelFont = DrawTheme.Mono(8.5f);
        const int MagSize = 128, MagZoom = 8;

        public static Bitmap GrabScreen(Rectangle area)
        {
            var bmp = new Bitmap(area.Width, area.Height, PixelFormat.Format32bppArgb);
            using (var g = Graphics.FromImage(bmp))
                g.CopyFromScreen(area.Location, Point.Empty, area.Size, CopyPixelOperation.SourceCopy);
            return bmp;
        }

        public static CaptureResult FullScreen()
        {
            var v = SystemInformation.VirtualScreen;
            return new CaptureResult { Image = GrabScreen(v), Source = L.T("Pantalla completa") };
        }

        /// <summary>Same overlay, used to choose what to record: returns only the area (screen pixels) and its source.</summary>
        public static CaptureResult SelectArea()
        {
            var v = SystemInformation.VirtualScreen;
            var wins = Native.VisibleWindows(IntPtr.Zero);
            using (var shot = GrabScreen(v))
            using (var f = new CaptureOverlay(shot, v, wins))
            {
                f.recordHint = true;
                f.ShowDialog();
                var r = f.chosen;
                r.Intersect(v);
                if (r.Width < 16 || r.Height < 16) return null;
                return new CaptureResult { Area = r, Source = f.chosenSource };
            }
        }

        bool recordHint;

        public static CaptureResult Run()
        {
            var v = SystemInformation.VirtualScreen;
            var wins = Native.VisibleWindows(IntPtr.Zero);
            using (var shot = GrabScreen(v))
            using (var f = new CaptureOverlay(shot, v, wins))
            {
                f.ShowDialog();
                if (f.chosen.Width < 1 || f.chosen.Height < 1) return null;
                var local = f.chosen;
                local.Offset(-v.X, -v.Y);
                local.Intersect(new Rectangle(Point.Empty, shot.Size));
                if (local.Width < 1 || local.Height < 1) return null;
                return new CaptureResult { Image = shot.Clone(local, PixelFormat.Format32bppArgb), Source = f.chosenSource };
            }
        }

        CaptureOverlay(Bitmap shot, Rectangle virtualScreen, List<KeyValuePair<IntPtr, Rectangle>> wins)
        {
            screen = shot;
            virt = virtualScreen;
            windows = wins;
            dim = new Bitmap(shot.Width, shot.Height, PixelFormat.Format32bppPArgb);
            using (var g = Graphics.FromImage(dim))
            {
                g.DrawImageUnscaled(shot, 0, 0);
                using (var b = new SolidBrush(Color.FromArgb(110, 0, 0, 0))) g.FillRectangle(b, 0, 0, shot.Width, shot.Height);
            }
            FormBorderStyle = FormBorderStyle.None;
            StartPosition = FormStartPosition.Manual;
            ShowInTaskbar = false;
            TopMost = true;
            AutoScaleMode = AutoScaleMode.None;
            Cursor = Cursors.Cross;
            KeyPreview = true;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.Opaque, true);
            Bounds = virt;
        }

        protected override void OnShown(EventArgs e)
        {
            base.OnShown(e);
            Bounds = virt;
            Activate();
            mouse = PointToClient(Cursor.Position);
            UpdateHover();
            Invalidate();
        }

        Rectangle ToClient(Rectangle screenRect)
        {
            var r = screenRect;
            r.Offset(-virt.X, -virt.Y);
            return r;
        }

        void UpdateHover()
        {
            var sp = new Point(mouse.X + virt.X, mouse.Y + virt.Y);
            Rectangle found = Rectangle.Empty;
            IntPtr h = IntPtr.Zero;
            foreach (var kv in windows)
            {
                if (kv.Value.Contains(sp)) { found = kv.Value; h = kv.Key; break; }
            }
            if (found.IsEmpty)
            {
                foreach (var s in Screen.AllScreens)
                    if (s.Bounds.Contains(sp)) { found = s.Bounds; break; }
            }
            found.Intersect(virt);
            var newHover = ToClient(found);
            if (newHover != hover)
            {
                InvalidateFrame(hover);
                hover = newHover;
                hoverHandle = h;
                InvalidateFrame(hover);
            }
        }

        void InvalidateFrame(Rectangle r)
        {
            if (r.IsEmpty) return;
            Invalidate(Rectangle.Inflate(r, 4, 4));
            Invalidate(LabelRect(r));
        }

        Rectangle LabelRect(Rectangle r)
        {
            int y = r.Top - 24;
            if (y < 0) y = r.Top + 4;
            return new Rectangle(r.Left, y, 280, 22);
        }

        Rectangle MagRect()
        {
            int x = mouse.X + 24, y = mouse.Y + 24;
            if (x + MagSize > ClientSize.Width) x = mouse.X - 24 - MagSize;
            if (y + MagSize + 22 > ClientSize.Height) y = mouse.Y - 24 - MagSize - 22;
            return new Rectangle(x, y, MagSize, MagSize + 22);
        }

        void InvalidateCursorStuff()
        {
            Invalidate(new Rectangle(0, mouse.Y - 1, ClientSize.Width, 3));
            Invalidate(new Rectangle(mouse.X - 1, 0, 3, ClientSize.Height));
            Invalidate(Rectangle.Inflate(MagRect(), 2, 2));
        }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            InvalidateCursorStuff();
            mouse = e.Location;
            if (dragging)
            {
                if (Math.Abs(mouse.X - start.X) > 3 || Math.Abs(mouse.Y - start.Y) > 3) moved = true;
                var old = sel;
                sel = Rectangle.FromLTRB(Math.Min(start.X, mouse.X), Math.Min(start.Y, mouse.Y),
                                         Math.Max(start.X, mouse.X) + 1, Math.Max(start.Y, mouse.Y) + 1);
                InvalidateFrame(old);
                InvalidateFrame(sel);
                if (moved && !hover.IsEmpty) { InvalidateFrame(hover); hover = Rectangle.Empty; }
            }
            else UpdateHover();
            InvalidateCursorStuff();
        }

        protected override void OnMouseDown(MouseEventArgs e)
        {
            if (e.Button == MouseButtons.Right) { chosen = Rectangle.Empty; Close(); return; }
            if (e.Button != MouseButtons.Left) return;
            dragging = true;
            moved = false;
            start = e.Location;
            sel = Rectangle.Empty;
        }

        protected override void OnMouseUp(MouseEventArgs e)
        {
            if (!dragging || e.Button != MouseButtons.Left) return;
            dragging = false;
            Rectangle pick;
            if (moved && sel.Width > 1 && sel.Height > 1)
            {
                pick = sel;
                chosenSource = L.T("Región");
            }
            else
            {
                pick = hover;
                string t = hoverHandle != IntPtr.Zero ? Native.WindowTitle(hoverHandle) : "";
                chosenSource = string.IsNullOrEmpty(t) ? L.T("Ventana") : t;
            }
            pick.Offset(virt.X, virt.Y);
            chosen = pick;
            Close();
        }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            if (e.KeyCode == Keys.Escape) { chosen = Rectangle.Empty; Close(); }
            else if (e.KeyCode == Keys.Enter && !hover.IsEmpty)
            {
                var r = hover; r.Offset(virt.X, virt.Y);
                chosen = r;
                string t = hoverHandle != IntPtr.Zero ? Native.WindowTitle(hoverHandle) : "";
                chosenSource = string.IsNullOrEmpty(t) ? L.T("Ventana") : t;
                Close();
            }
            else if (e.KeyCode == Keys.F)
            {
                chosen = virt;
                chosenSource = L.T("Pantalla completa");
                Close();
            }
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            var clip = e.ClipRectangle;
            g.InterpolationMode = InterpolationMode.NearestNeighbor;
            g.PixelOffsetMode = PixelOffsetMode.Half;
            g.CompositingMode = CompositingMode.SourceCopy;
            g.DrawImage(dim, clip, clip, GraphicsUnit.Pixel);

            Rectangle active = dragging && moved ? sel : hover;
            if (!active.IsEmpty)
            {
                var r = Rectangle.Intersect(active, clip);
                if (!r.IsEmpty) g.DrawImage(screen, r, r, GraphicsUnit.Pixel);
            }
            g.CompositingMode = CompositingMode.SourceOver;
            g.PixelOffsetMode = PixelOffsetMode.Default;

            // crosshair
            using (var p = new Pen(Color.FromArgb(160, 255, 255, 255), 1) { DashStyle = DashStyle.Dash })
            {
                g.DrawLine(p, 0, mouse.Y, ClientSize.Width, mouse.Y);
                g.DrawLine(p, mouse.X, 0, mouse.X, ClientSize.Height);
            }

            if (!active.IsEmpty)
            {
                Color c = DrawTheme.Accent;
                using (var p = new Pen(c, 2)) g.DrawRectangle(p, active.X, active.Y, active.Width - 1, active.Height - 1);
                var lr = LabelRect(active);
                string txt = (recordHint ? "● REC  " : "") + active.Width + " × " + active.Height + (dragging && moved ? "" : "   " + L.T("clic = ventana") + (recordHint ? "  ·  F = " + L.T("pantalla") : ""));
                using (var b = new SolidBrush(DrawTheme.A(DrawTheme.Header, 0.92))) g.FillRectangle(b, lr.X, lr.Y, TextRenderer.MeasureText(txt, labelFont).Width + 8, lr.Height);
                TextRenderer.DrawText(g, txt, labelFont, new Point(lr.X + 4, lr.Y + 3), DrawTheme.Ink);
            }

            DrawMagnifier(g);
        }

        void DrawMagnifier(Graphics g)
        {
            var mr = MagRect();
            int n = MagSize / MagZoom;
            var src = new Rectangle(mouse.X - n / 2, mouse.Y - n / 2, n, n);
            g.FillRectangle(Brushes.Black, mr.X, mr.Y, MagSize, MagSize);
            g.InterpolationMode = InterpolationMode.NearestNeighbor;
            g.PixelOffsetMode = PixelOffsetMode.Half;
            g.DrawImage(screen, new Rectangle(mr.X, mr.Y, MagSize, MagSize), src, GraphicsUnit.Pixel);
            g.PixelOffsetMode = PixelOffsetMode.Default;
            int cx = mr.X + (n / 2) * MagZoom, cy = mr.Y + (n / 2) * MagZoom;
            using (var p = new Pen(DrawTheme.Accent))
                g.DrawRectangle(p, cx, cy, MagZoom, MagZoom);
            using (var p = new Pen(DrawTheme.Accent)) g.DrawRectangle(p, mr.X, mr.Y, MagSize - 1, MagSize - 1);
            Color px = Color.Black;
            if (mouse.X >= 0 && mouse.Y >= 0 && mouse.X < screen.Width && mouse.Y < screen.Height) px = screen.GetPixel(mouse.X, mouse.Y);
            string info = (mouse.X + virt.X) + "," + (mouse.Y + virt.Y) + "  #" + px.R.ToString("X2") + px.G.ToString("X2") + px.B.ToString("X2");
            using (var b = new SolidBrush(DrawTheme.Header)) g.FillRectangle(b, mr.X, mr.Y + MagSize, MagSize, 22);
            TextRenderer.DrawText(g, info, labelFont, new Rectangle(mr.X, mr.Y + MagSize, MagSize, 22), DrawTheme.Muted,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing) { dim.Dispose(); labelFont.Dispose(); }
            base.Dispose(disposing);
        }
    }
}
