/// Generates a styled HTML email for sending a verification code.
///
/// This function creates a responsive HTML email body that includes:
/// 1. A hidden preheader for email client previews.
/// 2. The project's branding.
/// 3. The verification code, displayed prominently.
/// 4. A professional and clean design using inline CSS for maximum compatibility.
///
/// # Arguments
///
/// * `code` - A string slice that holds the verification code to be embedded in the email.
///
/// # Returns
///
/// A `String` containing the full HTML content of the email.
pub fn generate_verification_email_html(code: &str) -> String {
    use super::constant::VERIFICATION_CODE_EXPIRY;
    let current_year = time::OffsetDateTime::now_utc().year();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Project Contigo Verification Code</title>
</head>
<body style="margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif, 'Apple Color Emoji', 'Segoe UI Emoji', 'Segoe UI Symbol'; background-color: #f0f2f5;">

    <!-- This is a hidden preheader text. -->
    <div style="display:none;font-size:1px;color:#ffffff;line-height:1px;max-height:0px;max-width:0px;opacity:0;overflow:hidden;">
        Your verification code is: {code}
    </div>

    <table width="100%" border="0" cellspacing="0" cellpadding="0" style="background-color: #f0f2f5;">
        <tr>
            <td align="center" style="padding: 20px;">
                <!-- Main Content Table -->
                <table width="600" border="0" cellspacing="0" cellpadding="0" style="max-width: 600px; width: 100%; background-color: #ffffff; border-radius: 12px; box-shadow: 0 4px 12px rgba(0,0,0,0.08);">

                    <!-- Header Section -->
                    <tr>
                        <td align="center" style="padding: 40px 20px 20px 20px;">
                            <h1 style="margin: 0; color: #1c1e21; font-size: 32px; font-weight: 600;">🎉 Welcome 🥳</h1>
                            <p style="margin: 4px 0 0 0; color: #606770; font-size: 14px;">to Project Contigo</p>
                        </td>
                    </tr>

                    <!-- Body Section -->
                    <tr>
                        <td style="padding: 20px 40px;">
                            <h2 style="margin: 0 0 24px 0; font-size: 22px; font-weight: 600; color: #1c1e21; text-align: center;">Confirm Your Email Address</h2>
                            <p style="margin: 0 0 24px 0; font-size: 16px; line-height: 1.6; color: #606770; text-align: center;">
                                Thanks for signing up for Project Contigo! Please use the following code to complete your registration.
                            </p>

                            <!-- Verification Code Box -->
                            <div style="background-color: #e7f3ff; border-radius: 8px; padding: 16px; text-align: center;">
                                <p style="margin: 0; font-size: 36px; font-weight: 700; color: #1877f2; letter-spacing: 5px; line-height: 1.2;">
                                    {code}
                                </p>
                            </div>

                            <p style="margin: 24px 0 0 0; font-size: 14px; color: #606770; text-align: center;">
                                This code will expire in {} minutes. If you did not request this, please disregard this email.
                            </p>
                        </td>
                    </tr>

                    <!-- Footer Section -->
                    <tr>
                        <td align="center" style="padding: 30px 40px; border-top: 1px solid #e1e4e8;">
                            <p style="margin: 0; font-size: 12px; color: #90949c; line-height: 1.5;">
                                &copy; {current_year} Project Contigo. All rights reserved.<br>
                                This email was sent to you as part of the account verification process.
                            </p>
                        </td>
                    </tr>
                </table>
            </td>
        </tr>
    </table>
</body>
</html>"#,
        VERIFICATION_CODE_EXPIRY.as_secs() / 60,
    )
}
