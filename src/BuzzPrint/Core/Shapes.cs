#nullable disable
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Globalization;
using System.Linq;

namespace BuzzPrint
{
    public enum Tool { Select, Arrow, Line, Rect, Ellipse, Highlight, Pen, Text, Step, Blur, Crop }

    /// <summary>An annotation drawn on top of the capture, in image pixel coordinates.</summary>
    public class Mark
    {
        public Tool Kind;
        public Color Color = Color.Red;
        public float Width = 4;
        public List<PointF> Pts = new List<PointF>();
        public string Text = "";
        public float FontSize = 18;
        public int Number;

        public PointF P0 { get { return Pts[0]; } set { Pts[0] = value; } }
        public PointF P1 { get { return Pts[Pts.Count - 1]; } set { Pts[Pts.Count - 1] = value; } }

        public bool IsTwoPoint
        {
            get { return Kind == Tool.Arrow || Kind == Tool.Line || Kind == Tool.Rect || Kind == Tool.Ellipse || Kind == Tool.Highlight || Kind == Tool.Blur; }
        }

        public Mark Clone()
        {
            var s = (Mark)MemberwiseClone();
            s.Pts = new List<PointF>(Pts);
            return s;
        }

        public RectangleF Norm()
        {
            var a = P0; var b = P1;
            return RectangleF.FromLTRB(Math.Min(a.X, b.X), Math.Min(a.Y, b.Y), Math.Max(a.X, b.X), Math.Max(a.Y, b.Y));
        }

        public float StepRadius { get { return Math.Max(14, FontSize * 0.9f); } }

        public static Font MakeFont(float size)
        {
            return new Font("Segoe UI", size, FontStyle.Bold, GraphicsUnit.Pixel);
        }

        public RectangleF Bounds()
        {
            switch (Kind)
            {
                case Tool.Text:
                    using (var f = MakeFont(FontSize))
                    using (var bmp = new Bitmap(1, 1))
                    using (var g = Graphics.FromImage(bmp))
                    {
                        var sz = g.MeasureString(string.IsNullOrEmpty(Text) ? " " : Text, f);
                        return new RectangleF(P0.X, P0.Y, sz.Width + 12, sz.Height + 8);
                    }
                case Tool.Step:
                    float r = StepRadius;
                    return new RectangleF(P0.X - r, P0.Y - r, 2 * r, 2 * r);
                case Tool.Pen:
                    float x0 = Pts.Min(p => p.X), y0 = Pts.Min(p => p.Y), x1 = Pts.Max(p => p.X), y1 = Pts.Max(p => p.Y);
                    return RectangleF.FromLTRB(x0, y0, x1, y1);
                default:
                    return Norm();
            }
        }

        static float DistToSegment(PointF p, PointF a, PointF b)
        {
            float dx = b.X - a.X, dy = b.Y - a.Y;
            float len2 = dx * dx + dy * dy;
            float t = len2 == 0 ? 0 : Math.Max(0, Math.Min(1, ((p.X - a.X) * dx + (p.Y - a.Y) * dy) / len2));
            float px = a.X + t * dx - p.X, py = a.Y + t * dy - p.Y;
            return (float)Math.Sqrt(px * px + py * py);
        }

        public bool Hit(PointF p, float tol)
        {
            tol += Width / 2;
            switch (Kind)
            {
                case Tool.Arrow:
                case Tool.Line:
                    return DistToSegment(p, P0, P1) <= tol;
                case Tool.Pen:
                    for (int i = 1; i < Pts.Count; i++) if (DistToSegment(p, Pts[i - 1], Pts[i]) <= tol) return true;
                    return false;
                case Tool.Rect:
                {
                    var r = Norm();
                    var outer = RectangleF.Inflate(r, tol, tol);
                    var inner = RectangleF.Inflate(r, -tol, -tol);
                    return outer.Contains(p) && !(inner.Width > 0 && inner.Height > 0 && inner.Contains(p));
                }
                case Tool.Ellipse:
                {
                    var r = Norm();
                    float rx = r.Width / 2, ry = r.Height / 2;
                    if (rx < 1 || ry < 1) return RectangleF.Inflate(r, tol, tol).Contains(p);
                    float cx = r.X + rx, cy = r.Y + ry;
                    double d = Math.Sqrt(Math.Pow((p.X - cx) / rx, 2) + Math.Pow((p.Y - cy) / ry, 2));
                    return Math.Abs(d - 1) * Math.Min(rx, ry) <= tol;
                }
                default:
                    return RectangleF.Inflate(Bounds(), tol / 2, tol / 2).Contains(p);
            }
        }

        public void Offset(float dx, float dy)
        {
            for (int i = 0; i < Pts.Count; i++) Pts[i] = new PointF(Pts[i].X + dx, Pts[i].Y + dy);
        }

        public void Draw(Graphics g, Bitmap baseImage)
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            switch (Kind)
            {
                case Tool.Arrow:
                    using (var p = new Pen(Color, Width))
                    {
                        float cap = Math.Max(3f, 12f / (float)Math.Sqrt(Width));
                        p.CustomEndCap = new AdjustableArrowCap(cap * 0.75f, cap, true);
                        p.StartCap = LineCap.Round;
                        g.DrawLine(p, P0, P1);
                    }
                    break;
                case Tool.Line:
                    using (var p = new Pen(Color, Width) { StartCap = LineCap.Round, EndCap = LineCap.Round })
                        g.DrawLine(p, P0, P1);
                    break;
                case Tool.Rect:
                    using (var p = new Pen(Color, Width) { LineJoin = LineJoin.Round })
                    {
                        var r = Norm();
                        g.DrawRectangle(p, r.X, r.Y, r.Width, r.Height);
                    }
                    break;
                case Tool.Ellipse:
                    using (var p = new Pen(Color, Width))
                        g.DrawEllipse(p, Norm());
                    break;
                case Tool.Highlight:
                    using (var b = new SolidBrush(Color.FromArgb(105, Color)))
                    {
                        g.SmoothingMode = SmoothingMode.None;
                        g.FillRectangle(b, Norm());
                    }
                    break;
                case Tool.Pen:
                    if (Pts.Count > 1)
                        using (var p = new Pen(Color, Width) { StartCap = LineCap.Round, EndCap = LineCap.Round, LineJoin = LineJoin.Round })
                            g.DrawLines(p, Pts.ToArray());
                    break;
                case Tool.Text:
                    DrawText(g);
                    break;
                case Tool.Step:
                {
                    float r = StepRadius;
                    using (var b = new SolidBrush(Color)) g.FillEllipse(b, P0.X - r, P0.Y - r, 2 * r, 2 * r);
                    using (var p = new Pen(Color.White, 2)) g.DrawEllipse(p, P0.X - r, P0.Y - r, 2 * r, 2 * r);
                    using (var f = MakeFont(r * 1.1f))
                    using (var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center })
                        g.DrawString(Number.ToString(), f, Brushes.White, new RectangleF(P0.X - r, P0.Y - r + 1, 2 * r, 2 * r), sf);
                    break;
                }
                case Tool.Blur:
                    DrawPixelate(g, baseImage);
                    break;
            }
        }

        void DrawText(Graphics g)
        {
            var bounds = Bounds();
            g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
            using (var bg = new SolidBrush(Color.FromArgb(235, 255, 255, 255)))
            using (var path = RoundRect(bounds, 6))
            {
                g.FillPath(bg, path);
                using (var p = new Pen(Color, 2)) g.DrawPath(p, path);
            }
            using (var f = MakeFont(FontSize))
            using (var b = new SolidBrush(Color))
                g.DrawString(Text, f, b, P0.X + 6, P0.Y + 4);
        }

        void DrawPixelate(Graphics g, Bitmap src)
        {
            var r = Rectangle.Round(Norm());
            r.Intersect(new Rectangle(0, 0, src.Width, src.Height));
            if (r.Width < 2 || r.Height < 2) return;
            int block = Math.Max(6, (int)(Width * 2.5f));
            int sw = Math.Max(1, r.Width / block), sh = Math.Max(1, r.Height / block);
            using (var small = new Bitmap(sw, sh))
            {
                using (var gs = Graphics.FromImage(small))
                {
                    gs.InterpolationMode = InterpolationMode.HighQualityBilinear;
                    gs.DrawImage(src, new Rectangle(0, 0, sw, sh), r, GraphicsUnit.Pixel);
                }
                var st = g.Save();
                g.InterpolationMode = InterpolationMode.NearestNeighbor;
                g.PixelOffsetMode = PixelOffsetMode.Half;
                g.SmoothingMode = SmoothingMode.None;
                g.DrawImage(small, r, new Rectangle(0, 0, sw, sh), GraphicsUnit.Pixel);
                g.Restore(st);
            }
        }

        public static GraphicsPath RoundRect(RectangleF r, float rad)
        {
            var p = new GraphicsPath();
            float d = rad * 2;
            p.AddArc(r.X, r.Y, d, d, 180, 90);
            p.AddArc(r.Right - d, r.Y, d, d, 270, 90);
            p.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
            p.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
            p.CloseFigure();
            return p;
        }

        // ---- persistence (plain dictionaries for JavaScriptSerializer) ----
        public Dictionary<string, object> ToDict()
        {
            var inv = CultureInfo.InvariantCulture;
            return new Dictionary<string, object>
            {
                { "k", Kind.ToString() },
                { "c", Color.ToArgb() },
                { "w", Width.ToString(inv) },
                { "p", string.Join(" ", Pts.Select(p => p.X.ToString(inv) + "," + p.Y.ToString(inv))) },
                { "t", Text ?? "" },
                { "f", FontSize.ToString(inv) },
                { "n", Number }
            };
        }

        public static Mark FromDict(Dictionary<string, object> d)
        {
            var inv = CultureInfo.InvariantCulture;
            var s = new Mark();
            s.Kind = (Tool)Enum.Parse(typeof(Tool), d["k"].ToString());
            s.Color = Color.FromArgb(Convert.ToInt32(d["c"]));
            s.Width = float.Parse(d["w"].ToString(), inv);
            s.Text = d.ContainsKey("t") && d["t"] != null ? d["t"].ToString() : "";
            s.FontSize = float.Parse(d["f"].ToString(), inv);
            s.Number = Convert.ToInt32(d["n"]);
            foreach (var pair in d["p"].ToString().Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries))
            {
                var xy = pair.Split(',');
                s.Pts.Add(new PointF(float.Parse(xy[0], inv), float.Parse(xy[1], inv)));
            }
            return s;
        }

        public static Bitmap Render(Bitmap baseImage, IEnumerable<Mark> shapes)
        {
            var bmp = new Bitmap(baseImage.Width, baseImage.Height, PixelFormat.Format32bppArgb);
            using (var g = Graphics.FromImage(bmp))
            {
                g.DrawImageUnscaled(baseImage, 0, 0);
                foreach (var s in shapes) s.Draw(g, baseImage);
            }
            return bmp;
        }
    }
}
