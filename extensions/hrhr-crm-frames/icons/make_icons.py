#!/usr/bin/env python3
"""Generate Chrome extension icons with a window/frame design."""

from PIL import Image, ImageDraw
import os

def draw_frame_icon(size):
    """Draw a frame icon at the specified size (4x the final size for anti-aliasing)."""
    # Create image with transparent background
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Dark blue color for the rounded square background
    dark_blue = (29, 78, 216, 255)  # #1d4ed8
    white = (255, 255, 255, 255)

    # Draw rounded square background
    # Calculate radius proportional to size (roughly 20% of size for rounded corners)
    radius = max(1, size // 5)
    bbox = [0, 0, size - 1, size - 1]
    draw.rounded_rectangle(bbox, radius=radius, fill=dark_blue)

    # Draw window frame - outer rectangle (white outline)
    # Position with some padding
    padding = size // 6
    frame_outer = [padding, padding, size - padding - 1, size - padding - 1]
    draw.rectangle(frame_outer, outline=white, width=max(1, size // 20))

    # Draw inner filled rectangle (representing the page content)
    inner_padding = padding + size // 10
    inner_rect = [inner_padding, inner_padding, size - inner_padding - 1, size - inner_padding - 1]
    draw.rectangle(inner_rect, fill=white)

    return img

def generate_icons():
    """Generate all required icon sizes."""
    sizes = {
        'icon16.png': 16,
        'icon32.png': 32,
        'icon48.png': 48,
        'icon128.png': 128,
    }

    icon_dir = os.path.dirname(os.path.abspath(__file__))

    print("Generating Chrome extension icons...")
    print()

    for filename, size in sizes.items():
        # Draw at 4x size for anti-aliasing
        large_size = size * 4
        large_img = draw_frame_icon(large_size)

        # Downsample with LANCZOS for anti-aliasing
        final_img = large_img.resize((size, size), Image.Resampling.LANCZOS)

        # For the 128 store icon, center in a 128x128 canvas with transparent padding
        if size == 128:
            # Artwork should be in the central 96x96
            artwork_size = 96
            canvas = Image.new('RGBA', (128, 128), (0, 0, 0, 0))
            # Position artwork in the center
            offset = (128 - artwork_size) // 2
            artwork = draw_frame_icon(artwork_size * 4)
            artwork = artwork.resize((artwork_size, artwork_size), Image.Resampling.LANCZOS)
            canvas.paste(artwork, (offset, offset), artwork)
            final_img = canvas

        # Save PNG
        filepath = os.path.join(icon_dir, filename)
        final_img.save(filepath, 'PNG')

        # Get file size
        file_size = os.path.getsize(filepath)
        print(f"✓ {filename}: {size}x{size} ({file_size} bytes)")

    print()
    print("Icons generated successfully!")

if __name__ == '__main__':
    generate_icons()
