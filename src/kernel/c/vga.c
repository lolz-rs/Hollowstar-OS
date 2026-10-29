/*
 * Hollowstar VGA text-mode console.
 * 80x25, 16-color text, hardware cursor. Freestanding C, no libc.
 */

typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;

#define VGA_WIDTH  80
#define VGA_HEIGHT 25
#define VGA_MEM    ((volatile u16 *)0xB8000)

static u8 cursor_x;
static u8 cursor_y;
static u8 text_color = 0x0F;

static inline void outb(u16 port, u8 value)
{
    __asm__ volatile("outb %0, %1" : : "a"(value), "Nd"(port));
}

static void update_cursor(void)
{
    u16 pos = (u16)cursor_y * VGA_WIDTH + cursor_x;
    outb(0x3D4, 0x0F);
    outb(0x3D5, (u8)(pos & 0xFF));
    outb(0x3D4, 0x0E);
    outb(0x3D5, (u8)(pos >> 8));
}

static void clear_row(u32 row)
{
    u16 blank = (u16)(' ' | ((u16)text_color << 8));
    volatile u16 *line = VGA_MEM + row * VGA_WIDTH;
    u32 i;
    for (i = 0; i < VGA_WIDTH; i++)
        line[i] = blank;
}

static void scroll(void)
{
    u32 i;
    for (i = 0; i < (VGA_WIDTH * (VGA_HEIGHT - 1)); i++)
        VGA_MEM[i] = VGA_MEM[i + VGA_WIDTH];
    clear_row(VGA_HEIGHT - 1);
    cursor_y = VGA_HEIGHT - 1;
}

static void newline(void)
{
    cursor_x = 0;
    if (cursor_y + 1 >= VGA_HEIGHT)
        scroll();
    else
        cursor_y++;
}

void vga_clear(void)
{
    u32 i;
    u16 blank = (u16)(' ' | ((u16)text_color << 8));
    for (i = 0; i < VGA_WIDTH * VGA_HEIGHT; i++)
        VGA_MEM[i] = blank;
    cursor_x = 0;
    cursor_y = 0;
    update_cursor();
}

void vga_set_color(u8 color)
{
    text_color = color;
}

u8 vga_get_color(void)
{
    return text_color;
}

void vga_putc(char c)
{
    switch (c) {
    case '\n':
        newline();
        break;
    case '\r':
        cursor_x = 0;
        break;
    case '\b':
        if (cursor_x > 0) {
            cursor_x--;
        } else if (cursor_y > 0) {
            cursor_y--;
            cursor_x = VGA_WIDTH - 1;
        } else {
            break;
        }
        VGA_MEM[(u32)cursor_y * VGA_WIDTH + cursor_x] =
            (u16)(' ' | ((u16)text_color << 8));
        break;
    case '\t': {
        u8 next = (u8)((cursor_x + 8) & ~7u);
        while (cursor_x < next) {
            if (cursor_x >= VGA_WIDTH)
                newline();
            else {
                VGA_MEM[(u32)cursor_y * VGA_WIDTH + cursor_x] =
                    (u16)(' ' | ((u16)text_color << 8));
                cursor_x++;
            }
        }
        break;
    }
    default:
        VGA_MEM[(u32)cursor_y * VGA_WIDTH + cursor_x] =
            (u16)((u8)c | ((u16)text_color << 8));
        cursor_x++;
        if (cursor_x >= VGA_WIDTH)
            newline();
        break;
    }
    update_cursor();
}

void vga_write(const char *s, u32 len)
{
    u32 i;
    for (i = 0; i < len; i++)
        vga_putc(s[i]);
}
