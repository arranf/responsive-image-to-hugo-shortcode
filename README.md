# responsivebreakpoints.com Image to Hugo Template


This program is used to produce a Hugo shortcode from the input of a .zip file and HTML from [responsivebreakpints.com](https://responsivebreakpoints.com)

It does this in three steps:
1. Writing to the [Hugo images data template](https://gohugo.io/templates/data-templates/)
2. Providing a shortcode that can be copy-pasted with values autofilled
3. Uploading images in a .zip file to S3

I go into detail on the reasons behind this program [in this blog post](https://blog.arranfrance.com/post/responsive-blog-images/). A Rust port of [this program](https://github.com/arranf/ResponsiveImagetoShortcode).