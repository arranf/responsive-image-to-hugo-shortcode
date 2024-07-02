# Upload Responsive Images for Hugo

## Summary

This is the third iteration of my efforts to make working with [responsive images](https://css-tricks.com/a-guide-to-the-responsive-images-syntax-in-html/) in [Hugo](https://gohugo.io/) easier.

This program:

1. Takes an image (or directory of images) as input
2. Converts each input image to `.webp` (whilst preserving its orientation).
3. Creates resized versions of each input image suitable for different screen sizes.
4. Uploads all image versions to S3.
5. Generates a [srcset](https://css-tricks.com/a-guide-to-the-responsive-images-syntax-in-html/#using-srcset) and [sizes](https://css-tricks.com/a-guide-to-the-responsive-images-syntax-in-html/#aa-using-srcset-w-sizes) attribute for each input image
5. Creates a [Hugo data file](https://gohugo.io/templates/data-templates/) with JSON formatted data for each image.
6. Outputs either a prefilled [shortcode](https://gohugo.io/content-management/shortcodes/) to copy and paste or a YAML formatted list of the data keys.

## Usage

```sh
responsive-image-to-hugo-template -o ./test/images.json ./test/example_zip.zip ./test/example_input.txt --name Test
```

## Directories

* build/ - A compiled copy of [SQIP FFI](https://github.com/arranf/sqip-ffi)
* test - Test inputs
* build.rs - A custom build script

## Appendix

I go into detail on the reasons behind this program [in this blog post](https://blog.arranfrance.com/post/responsive-blog-images/). This was originally a Rust port of [this program](https://github.com/arranf/ResponsiveImagetoShortcode), however my needs have since evolved. The v1 tag in this repository best represents the last "port" version of this program. Commits after the tag have significantly diverged from the original intent.